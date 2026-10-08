use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::mpsc;

use crate::adb::{
    AndroidDevice, ConnectionType, DeviceInfo, DeviceState, TermuxSource as InstalledTermuxSource,
};
use crate::error::{AppError, AppResult};
use crate::provision::{
    build_plan, bundled_recipes, run_plan, AndroidProvisionExecutor, ProvisionEvent, ProvisionPlan,
    ProvisionProgressStore, ProvisionRecipe, ProvisionStepId, ProvisionStepState, MIN_FREE_BYTES,
};
use crate::state::AppState;

#[tauri::command]
pub fn list_provision_recipes(state: State<'_, AppState>) -> AppResult<Vec<ProvisionRecipe>> {
    list_recipes_from_directory(&state.data_dir)
}

fn recipe_directory(data_dir: &Path) -> PathBuf {
    data_dir.join("provision-recipes")
}

fn lock_provisioning_runs(
    runs: &Mutex<std::collections::HashSet<String>>,
) -> AppResult<MutexGuard<'_, std::collections::HashSet<String>>> {
    runs.lock().map_err(|_| {
        AppError::Config("Provisioning state is unavailable after a previous failure.".into())
    })
}

fn remove_active_provisioning_run(
    runs: &Mutex<std::collections::HashSet<String>>,
    device_id: &str,
) -> AppResult<()> {
    lock_provisioning_runs(runs)?.remove(device_id);
    Ok(())
}

fn save_user_recipe(data_dir: &Path, source: &str) -> AppResult<ProvisionRecipe> {
    let recipe = ProvisionRecipe::parse(source).map_err(AppError::Config)?;
    if bundled_recipes()
        .map_err(AppError::Config)?
        .iter()
        .any(|bundled| bundled.id == recipe.id)
    {
        return Err(AppError::Config(
            "Bundled recipes are read-only; save this recipe with a new id.".into(),
        ));
    }
    let directory = recipe_directory(data_dir);
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{}.toml", recipe.id));
    let temporary = path.with_extension("toml.tmp");
    std::fs::write(&temporary, source)?;
    crate::platform::restrict_file(&temporary)?;
    std::fs::rename(temporary, path)?;
    Ok(recipe)
}

fn preflight_failure(
    device: &AndroidDevice,
    info: &DeviceInfo,
    recipe: &ProvisionRecipe,
) -> Option<String> {
    if device.state != DeviceState::Device {
        return Some("Connect and authorize this phone before provisioning.".into());
    }
    if device.connection == ConnectionType::Usb {
        return Some("Provisioning requires a Wireless ADB connection.".into());
    }
    if info
        .cpu
        .as_ref()
        .and_then(|cpu| cpu.abi.as_deref())
        .is_none()
    {
        return Some("The phone's CPU ABI could not be detected.".into());
    }
    match &info.storage {
        Some(storage) if storage.free_bytes < MIN_FREE_BYTES => {
            return Some("Free at least 2 GB on the phone before provisioning.".into());
        }
        None => {
            return Some(
                "Free phone storage could not be checked; refresh device information first.".into(),
            );
        }
        _ => {}
    }
    if let Some(termux) = &info.termux {
        if termux.installed {
            let compatible = match recipe.termux_source {
                crate::provision::ProvisionTermuxSource::Fdroid => {
                    termux.source == InstalledTermuxSource::FDroid
                }
                crate::provision::ProvisionTermuxSource::Github => {
                    termux.source == InstalledTermuxSource::Sideloaded
                }
            };
            if !compatible {
                return Some(format!(
                    "Installed Termux source ({:?}) does not match recipe source ({:?}); replacing Termux deletes its data.",
                    termux.source, recipe.termux_source
                ));
            }
        }
    }
    None
}

fn recipe_by_id(data_dir: &Path, recipe_id: &str) -> AppResult<ProvisionRecipe> {
    list_recipes_from_directory(data_dir)?
        .into_iter()
        .find(|recipe| recipe.id == recipe_id)
        .ok_or_else(|| AppError::Config(format!("Unknown provisioning recipe: {recipe_id}")))
}

fn list_recipes_from_directory(data_dir: &Path) -> AppResult<Vec<ProvisionRecipe>> {
    let mut recipes = bundled_recipes().map_err(AppError::Config)?;
    let directory = recipe_directory(data_dir);
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(recipes),
        Err(error) => return Err(error.into()),
    };
    let bundled_ids = recipes
        .iter()
        .map(|recipe| recipe.id.clone())
        .collect::<std::collections::HashSet<_>>();
    for entry in entries.filter_map(Result::ok) {
        if entry
            .path()
            .extension()
            .is_none_or(|extension| extension != "toml")
        {
            continue;
        }
        if let Ok(source) = std::fs::read_to_string(entry.path()) {
            match ProvisionRecipe::parse(&source) {
                Ok(recipe) if !bundled_ids.contains(&recipe.id) => recipes.push(recipe),
                _ => {}
            }
        }
    }
    Ok(recipes)
}

#[tauri::command]
pub fn get_provision_recipe_source(
    state: State<'_, AppState>,
    recipe_id: String,
) -> AppResult<String> {
    let recipe = recipe_by_id(&state.data_dir, &recipe_id)?;
    let user_path = recipe_directory(&state.data_dir).join(format!("{}.toml", recipe.id));
    if user_path.is_file() {
        return Ok(std::fs::read_to_string(user_path)?);
    }
    toml::to_string_pretty(&recipe).map_err(|error| AppError::Io(error.to_string()))
}

#[tauri::command]
pub fn save_provision_recipe(
    state: State<'_, AppState>,
    source: String,
) -> AppResult<ProvisionRecipe> {
    save_user_recipe(&state.data_dir, &source)
}

#[tauri::command]
pub async fn import_provision_recipe<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> AppResult<Option<ProvisionRecipe>> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Provisioning recipes", &["toml"])
        .pick_file(move |path| {
            let _ = sender.send(path);
        });
    let Some(file_path) = receiver
        .await
        .map_err(|error| AppError::Io(error.to_string()))?
    else {
        return Ok(None);
    };
    let path = file_path
        .into_path()
        .map_err(|error| AppError::Io(error.to_string()))?;
    let source = std::fs::read_to_string(path)?;
    save_user_recipe(&state.data_dir, &source).map(Some)
}

#[tauri::command]
pub async fn export_provision_recipe<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    recipe_id: String,
) -> AppResult<Option<String>> {
    let recipe = recipe_by_id(&state.data_dir, &recipe_id)?;
    let source_path = recipe_directory(&state.data_dir).join(format!("{recipe_id}.toml"));
    let source = if source_path.is_file() {
        std::fs::read_to_string(source_path)?
    } else {
        toml::to_string_pretty(&recipe).map_err(|error| AppError::Io(error.to_string()))?
    };
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Provisioning recipes", &["toml"])
        .set_file_name(format!("{}.toml", recipe.id))
        .save_file(move |path| {
            let _ = sender.send(path);
        });
    let Some(file_path) = receiver
        .await
        .map_err(|error| AppError::Io(error.to_string()))?
    else {
        return Ok(None);
    };
    let path = file_path
        .into_path()
        .map_err(|error| AppError::Io(error.to_string()))?;
    std::fs::write(&path, source)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}
#[tauri::command]
pub async fn get_provision_plan(
    state: State<'_, AppState>,
    serial: String,
    recipe_id: String,
) -> AppResult<ProvisionPlan> {
    let recipe = recipe_by_id(&state.data_dir, &recipe_id)?;
    let device_id = state.device_id_for(&serial);
    let store = ProvisionProgressStore::new(state.data_dir.clone());
    let progress = store.load(&device_id, &recipe.id)?;
    let mut plan = build_plan(&serial, &device_id, &recipe, &progress);

    let preflight = match state.devices.get(&serial) {
        Some(device) if device.state == DeviceState::Device => {
            let client = state.adb_client().await?;
            let mut info = client.device_info(&serial).await?;
            if let Some(installed) = info.termux.as_mut() {
                crate::provision::apk::apply_termux_install_receipt(
                    &client,
                    &serial,
                    &state.data_dir,
                    &device_id,
                    installed,
                )
                .await?;
            }
            preflight_failure(&device, &info, &recipe)
        }
        Some(_) => Some("Connect and authorize this phone before provisioning.".into()),
        None => Some("Connect and authorize this phone before provisioning.".into()),
    };
    if let Some(step) = plan
        .steps
        .iter_mut()
        .find(|step| step.id == ProvisionStepId::Preflight)
    {
        match preflight {
            Some(reason) => {
                step.state = ProvisionStepState::Blocked;
                step.detail = Some(reason);
            }
            None => {
                step.state = ProvisionStepState::Done;
                step.detail = None;
            }
        }
    }
    Ok(plan)
}

#[tauri::command]
pub fn reset_provision_progress(
    state: State<'_, AppState>,
    serial: String,
    recipe_id: String,
) -> AppResult<()> {
    let recipe = recipe_by_id(&state.data_dir, &recipe_id)?;
    let device_id = state.device_id_for(&serial);
    ProvisionProgressStore::new(state.data_dir.clone()).reset(&device_id, &recipe.id)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn run_provision<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    serial: String,
    recipe_id: String,
    from_step: Option<ProvisionStepId>,
    only_step: bool,
    approved_steps: Vec<ProvisionStepId>,
    on_event: Channel<ProvisionEvent>,
) -> AppResult<String> {
    let recipe = recipe_by_id(&state.data_dir, &recipe_id)?;
    let device_id = state.device_id_for(&serial);
    {
        let mut active = lock_provisioning_runs(&state.provisioning_runs)?;
        if !active.insert(device_id.clone()) {
            return Err(AppError::Config(
                "Provisioning is already running for this phone.".into(),
            ));
        }
    }

    let (run_id, cancel) = state.streams.register(&serial, "provision");
    let executor = AndroidProvisionExecutor::new(app.clone(), serial.clone(), recipe.clone());
    let store = ProvisionProgressStore::new(state.data_dir.clone());
    let progress = match store.load(&device_id, &recipe.id) {
        Ok(progress) => progress,
        Err(error) => {
            state.streams.finish(&run_id);
            if let Err(lock_error) =
                remove_active_provisioning_run(&state.provisioning_runs, &device_id)
            {
                tracing::error!(error = %lock_error, "failed to release provisioning run after progress load failure");
            }
            return Err(error);
        }
    };
    let mut plan = build_plan(&serial, &device_id, &recipe, &progress);
    let stream_id = run_id.clone();
    let event_cancel = cancel.clone();
    let (sender, mut receiver) = mpsc::channel(128);
    let forwarder = tauri::async_runtime::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if on_event.send(event).is_err() {
                event_cancel.cancel();
                break;
            }
        }
    });
    let active_device_id = device_id.clone();
    tauri::async_runtime::spawn(async move {
        let mut progress = progress;
        let result = run_plan(
            &executor,
            &mut plan,
            &mut progress,
            from_step,
            only_step,
            &approved_steps,
            cancel,
            sender,
            &store,
        )
        .await;
        if let Err(error) = result {
            tracing::warn!(device_id = %active_device_id, error = %error, "provisioning plan stopped");
        }
        forwarder.await.ok();
        app.state::<AppState>().streams.finish(&stream_id);
        if let Err(error) = remove_active_provisioning_run(
            &app.state::<AppState>().provisioning_runs,
            &active_device_id,
        ) {
            tracing::error!(device_id = %active_device_id, error = %error, "failed to release completed provisioning run");
        }
    });
    Ok(run_id)
}

#[tauri::command]
pub fn cancel_provision(state: State<'_, AppState>, run_id: String) -> bool {
    state.streams.cancel(&run_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::{BatteryInfo, CpuInfo, MemoryInfo, StorageInfo};

    #[test]
    fn poisoned_provisioning_run_lock_returns_error() {
        let runs = std::sync::Arc::new(Mutex::new(std::collections::HashSet::new()));
        let poison = runs.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poison.lock().unwrap();
            panic!("poison provisioning runs");
        })
        .join();

        let error = lock_provisioning_runs(&runs).unwrap_err();
        assert!(error
            .to_string()
            .contains("Provisioning state is unavailable"));
    }

    fn bundled(id: &str) -> ProvisionRecipe {
        bundled_recipes()
            .unwrap()
            .into_iter()
            .find(|recipe| recipe.id == id)
            .unwrap()
    }

    fn temp_data_dir() -> PathBuf {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hacc-recipe-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn user_recipe_save_list_and_load_round_trip() {
        let data_dir = temp_data_dir();
        let mut recipe = bundled("debian-official");
        recipe.id = "my-debian-recipe".into();
        recipe.name = "My Debian recipe".into();
        let source = toml::to_string(&recipe).unwrap();

        let saved = save_user_recipe(&data_dir, &source).unwrap();
        assert_eq!(saved, recipe);
        assert!(list_recipes_from_directory(&data_dir)
            .unwrap()
            .iter()
            .any(|item| item.id == "my-debian-recipe"));
        assert_eq!(recipe_by_id(&data_dir, "my-debian-recipe").unwrap(), recipe);
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn user_recipe_cannot_overwrite_a_bundled_recipe() {
        let data_dir = temp_data_dir();
        let bundled_recipe = bundled("debian-official");
        let error =
            save_user_recipe(&data_dir, &toml::to_string(&bundled_recipe).unwrap()).unwrap_err();
        assert!(error.user_message().contains("read-only"));
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    fn device(connection: ConnectionType) -> AndroidDevice {
        AndroidDevice {
            serial: "192.0.2.10:5555".into(),
            device_id: Some("device-id".into()),
            model: Some("test".into()),
            product: None,
            device: None,
            transport_id: None,
            ip_address: Some("192.0.2.10".into()),
            state: DeviceState::Device,
            raw_state: "device".into(),
            connection,
        }
    }

    fn info() -> DeviceInfo {
        DeviceInfo {
            serial: "192.0.2.10:5555".into(),
            device_id: Some("device-id".into()),
            manufacturer: Some("Test".into()),
            model: Some("Phone".into()),
            android_version: Some("14".into()),
            sdk: Some(34),
            ip_address: Some("192.0.2.10".into()),
            battery: Some(BatteryInfo {
                level: Some(80),
                charging: true,
                status: "charging".into(),
                plugged: Some("ac".into()),
                temperature_c: None,
            }),
            storage: Some(StorageInfo {
                total_bytes: 64 * 1024 * 1024 * 1024,
                free_bytes: 32 * 1024 * 1024 * 1024,
            }),
            memory: Some(MemoryInfo {
                total_bytes: 8 * 1024 * 1024 * 1024,
                available_bytes: 4 * 1024 * 1024 * 1024,
            }),
            cpu: Some(CpuInfo {
                abi: Some("arm64-v8a".into()),
                cores: Some(8),
                hardware: Some("test".into()),
            }),
            termux: None,
        }
    }

    #[test]
    fn preflight_rejects_usb_and_low_storage() {
        let recipe = bundled("debian-official");
        assert!(
            preflight_failure(&device(ConnectionType::Usb), &info(), &recipe)
                .unwrap()
                .contains("Wireless ADB")
        );

        let mut low_storage = info();
        low_storage.storage.as_mut().unwrap().free_bytes = MIN_FREE_BYTES - 1;
        assert!(
            preflight_failure(&device(ConnectionType::WirelessIp), &low_storage, &recipe)
                .unwrap()
                .contains("2 GB")
        );

        let mut unknown_storage = info();
        unknown_storage.storage = None;
        assert!(preflight_failure(
            &device(ConnectionType::WirelessIp),
            &unknown_storage,
            &recipe
        )
        .unwrap()
        .contains("storage could not be checked"));
    }

    #[test]
    fn preflight_rejects_missing_abi_and_termux_source_mismatch() {
        let recipe = bundled("debian-official");
        let mut missing_abi = info();
        missing_abi.cpu.as_mut().unwrap().abi = None;
        assert!(
            preflight_failure(&device(ConnectionType::WirelessIp), &missing_abi, &recipe)
                .unwrap()
                .contains("ABI")
        );

        let mut play_termux = info();
        play_termux.termux = Some(crate::adb::TermuxPackageInfo {
            installed: true,
            version_name: Some("0.118.0".into()),
            version_code: Some(1000),
            installer: Some("com.android.vending".into()),
            source: InstalledTermuxSource::PlayStore,
            companions: Vec::new(),
        });
        assert!(
            preflight_failure(&device(ConnectionType::WirelessIp), &play_termux, &recipe)
                .unwrap()
                .contains("deletes its data")
        );
    }

    #[test]
    fn preflight_accepts_ready_wireless_phone_with_matching_source() {
        let recipe = bundled("debian-official");
        let mut ready = info();
        ready.termux = Some(crate::adb::TermuxPackageInfo {
            installed: true,
            version_name: Some("0.118.0".into()),
            version_code: Some(1000),
            installer: Some("org.fdroid.fdroid".into()),
            source: InstalledTermuxSource::FDroid,
            companions: Vec::new(),
        });
        assert_eq!(
            preflight_failure(&device(ConnectionType::WirelessIp), &ready, &recipe),
            None
        );
    }
}
