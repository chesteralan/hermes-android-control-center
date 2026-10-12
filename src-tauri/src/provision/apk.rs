use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::error::{AppError, AppResult};
use crate::process::cancellable;
use tokio_util::sync::CancellationToken;

use super::recipe::ProvisionTermuxSource;

const FDROID_INDEX_URL: &str = "https://f-droid.org/repo/index-v1.jar";
const USER_AGENT: &str = "HermesControlCenter/0.1 (Termux provisioning)";
const MAX_APK_MIB: usize = 200;
const MAX_APK_BYTES: usize = MAX_APK_MIB * 1024 * 1024;
const DOWNLOAD_PROGRESS_INTERVAL_BYTES: u64 = 1024 * 1024;
const MAX_CHECKSUM_FILE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApkAsset {
    pub file_name: String,
    pub url: String,
    pub sha256: Option<String>,
    pub version_name: String,
    pub version_code: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedApk {
    pub path: PathBuf,
    pub file_name: String,
    pub version_name: String,
    pub version_code: u64,
    pub sha256: String,
    pub verified_against_source: bool,
}

struct AppApkRequest<'a> {
    package_name: &'a str,
    github_repository: &'a str,
    github_asset_prefix: &'a str,
    abi: &'a str,
    cache_file_name: &'a str,
}

#[derive(Serialize, Deserialize)]
struct TermuxInstallReceipt {
    source: ProvisionTermuxSource,
    sha256: String,
}

fn receipt_path(data_dir: &Path, device_id: &str) -> PathBuf {
    data_dir.join("provisioning").join(format!(
        "{}-termux-install.json",
        digest_hex(device_id.as_bytes())
    ))
}

pub fn record_termux_install(
    data_dir: &Path,
    device_id: &str,
    source: ProvisionTermuxSource,
    downloaded: &DownloadedApk,
) -> AppResult<()> {
    let path = receipt_path(data_dir, device_id);
    std::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| AppError::Io("No receipt directory".into()))?,
    )?;
    let receipt = TermuxInstallReceipt {
        source,
        sha256: downloaded.sha256.clone(),
    };
    let temporary = path.with_extension("tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec(&receipt).map_err(|error| AppError::Io(error.to_string()))?,
    )?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

pub async fn apply_termux_install_receipt(
    client: &crate::adb::AdbClient,
    serial: &str,
    data_dir: &Path,
    device_id: &str,
    installed: &mut crate::adb::TermuxPackageInfo,
) -> AppResult<()> {
    if !installed.installed || installed.source != crate::adb::TermuxSource::Sideloaded {
        return Ok(());
    }
    let bytes = match std::fs::read(receipt_path(data_dir, device_id)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let receipt: TermuxInstallReceipt = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Io(format!("Invalid Termux install receipt: {error}")))?;
    let output = client
        .shell(
            serial,
            "path=$(pm path com.termux | head -n 1); sha256sum \"${path#package:}\"",
            std::time::Duration::from_secs(60),
        )
        .await?;
    if output.success()
        && valid_sha256(&receipt.sha256)
        && output
            .stdout
            .split_whitespace()
            .next()
            .is_some_and(|digest| digest.eq_ignore_ascii_case(&receipt.sha256))
    {
        installed.source = match receipt.source {
            ProvisionTermuxSource::Fdroid => crate::adb::TermuxSource::FDroid,
            ProvisionTermuxSource::Github => crate::adb::TermuxSource::Sideloaded,
        };
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubRelease {
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FdroidPackage {
    apk_name: String,
    hash: String,
    hash_type: String,
    version_name: String,
    version_code: u64,
    #[serde(default, rename = "nativecode")]
    native_code: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FdroidIndex {
    packages: HashMap<String, Vec<serde_json::Value>>,
}

pub fn select_fdroid_asset(index_json: &str, abi: &str) -> AppResult<ApkAsset> {
    select_fdroid_app_asset(index_json, "com.termux", abi)
}

pub fn select_fdroid_app_asset(
    index_json: &str,
    package_name: &str,
    abi: &str,
) -> AppResult<ApkAsset> {
    let index: FdroidIndex = serde_json::from_str(index_json)
        .map_err(|error| AppError::Io(format!("Could not parse F-Droid package index: {error}")))?;
    let expected_abi = normalize_abi(abi)?;
    let package = index
        .packages
        .get(package_name)
        .ok_or_else(|| AppError::Config(format!("F-Droid index has no package {package_name}.")))?
        .iter()
        .cloned()
        .map(serde_json::from_value::<FdroidPackage>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::Io(format!(
                "Could not parse F-Droid metadata for {package_name}: {error}"
            ))
        })?
        .into_iter()
        .filter(|package| package.hash_type.eq_ignore_ascii_case("sha256"))
        .filter(|package| {
            package.apk_name
                == Path::new(&package.apk_name)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
        })
        .filter(|package| {
            package.native_code.is_empty()
                || package
                    .native_code
                    .iter()
                    .any(|native| normalize_abi(native).ok().as_deref() == Some(&expected_abi))
        })
        .max_by_key(|package| package.version_code)
        .ok_or_else(|| {
            AppError::Config(format!(
                "F-Droid has no SHA-256 indexed Termux APK for ABI {abi}."
            ))
        })?;
    if !valid_sha256(&package.hash) {
        return Err(AppError::Config(
            "F-Droid returned an invalid Termux APK SHA-256.".into(),
        ));
    }
    Ok(ApkAsset {
        file_name: package.apk_name.clone(),
        url: format!("https://f-droid.org/repo/{}", package.apk_name),
        sha256: Some(package.hash.to_ascii_lowercase()),
        version_name: package.version_name,
        version_code: package.version_code,
    })
}

pub fn select_github_asset(release_json: &str, abi: &str) -> AppResult<ApkAsset> {
    select_github_asset_for_prefix(release_json, "termux-app_", abi)
}

pub fn select_github_asset_for_prefix(
    release_json: &str,
    asset_prefix: &str,
    abi: &str,
) -> AppResult<ApkAsset> {
    let release: GithubRelease = serde_json::from_str(release_json)
        .map_err(|error| AppError::Io(format!("Could not parse Termux GitHub release: {error}")))?;
    let expected_abi = normalize_abi(abi)?;
    let asset = release
        .assets
        .into_iter()
        .filter(|asset| asset.name.starts_with(asset_prefix) && asset.name.ends_with(".apk"))
        .find(|asset| asset.name.contains(&expected_abi))
        .or_else(|| {
            serde_json::from_str::<GithubRelease>(release_json)
                .ok()?
                .assets
                .into_iter()
                .find(|asset| {
                    asset.name.starts_with(asset_prefix)
                        && asset.name.ends_with(".apk")
                        && asset.name.contains("universal")
                })
        })
        .ok_or_else(|| {
            AppError::Config(format!("Termux GitHub release has no APK for ABI {abi}."))
        })?;
    let sha256 = match asset.digest.as_deref() {
        None => None,
        Some(digest) => {
            let digest = digest.strip_prefix("sha256:").ok_or_else(|| {
                AppError::Config("GitHub returned an invalid Termux APK SHA-256.".into())
            })?;
            if !valid_sha256(digest) {
                return Err(AppError::Config(
                    "GitHub returned an invalid Termux APK SHA-256.".into(),
                ));
            }
            Some(digest.to_ascii_lowercase())
        }
    };
    let version_name = asset
        .name
        .strip_prefix(asset_prefix)
        .and_then(|name| name.strip_prefix('v'))
        .and_then(|name| name.split('+').next())
        .unwrap_or("unknown")
        .to_string();
    Ok(ApkAsset {
        file_name: asset.name,
        url: asset.browser_download_url,
        sha256,
        version_name,
        version_code: 0,
    })
}

pub fn sha256_from_checksum_file(checksum_text: &str, file_name: &str) -> Option<String> {
    let wanted = Path::new(file_name).file_name()?.to_string_lossy();
    checksum_text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let digest = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == wanted && valid_sha256(digest)).then(|| digest.to_ascii_lowercase())
    })
}

fn checksum_asset_url(release_json: &str) -> Option<String> {
    let release: GithubRelease = serde_json::from_str(release_json).ok()?;
    release
        .assets
        .into_iter()
        .find(|asset| {
            let name = asset.name.to_ascii_lowercase();
            !name.ends_with(".apk") && (name.contains("sha256") || name.contains("checksum"))
        })
        .map(|asset| asset.browser_download_url)
}

async fn fetch_text_capped(client: &reqwest::Client, url: &str) -> AppResult<String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| {
            AppError::Io(format!("Could not download release checksum file: {error}"))
        })?
        .error_for_status()
        .map_err(|error| AppError::Io(format!("Release checksum file request failed: {error}")))?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_CHECKSUM_FILE_BYTES as u64)
    {
        return Err(AppError::Config(
            "Release checksum file exceeds the 1 MiB safety limit.".into(),
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::Io(format!("Could not read release checksum file: {error}")))?;
    if bytes.len() > MAX_CHECKSUM_FILE_BYTES {
        return Err(AppError::Config(
            "Release checksum file exceeds the 1 MiB safety limit.".into(),
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub async fn download_termux_apk(
    source: ProvisionTermuxSource,
    abi: &str,
    cache_dir: &Path,
    cancel: CancellationToken,
    on_progress: impl FnMut(u64, Option<u64>),
) -> AppResult<DownloadedApk> {
    download_app_apk(
        source,
        AppApkRequest {
            package_name: "com.termux",
            github_repository: "termux/termux-app",
            github_asset_prefix: "termux-app_",
            abi,
            cache_file_name: "termux.apk",
        },
        cache_dir,
        cancel,
        on_progress,
    )
    .await
}

pub async fn download_termux_boot_apk(
    source: ProvisionTermuxSource,
    abi: &str,
    cache_dir: &Path,
    cancel: CancellationToken,
    on_progress: impl FnMut(u64, Option<u64>),
) -> AppResult<DownloadedApk> {
    download_app_apk(
        source,
        AppApkRequest {
            package_name: "com.termux.boot",
            github_repository: "termux/termux-boot",
            github_asset_prefix: "termux-boot_",
            abi,
            cache_file_name: "termux-boot.apk",
        },
        cache_dir,
        cancel,
        on_progress,
    )
    .await
}

async fn download_app_apk(
    source: ProvisionTermuxSource,
    request: AppApkRequest<'_>,
    cache_dir: &Path,
    cancel: CancellationToken,
    on_progress: impl FnMut(u64, Option<u64>),
) -> AppResult<DownloadedApk> {
    cancellable(
        &cancel,
        download_app_apk_inner(source, request, cache_dir, on_progress),
    )
    .await
}

async fn download_app_apk_inner(
    source: ProvisionTermuxSource,
    request: AppApkRequest<'_>,
    cache_dir: &Path,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> AppResult<DownloadedApk> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|error| AppError::Io(error.to_string()))?;
    let (mut asset, release_json) = match source {
        ProvisionTermuxSource::Fdroid => {
            let response = client
                .get(FDROID_INDEX_URL)
                .send()
                .await
                .map_err(|error| {
                    AppError::Io(format!("Could not download F-Droid index: {error}"))
                })?
                .error_for_status()
                .map_err(|error| AppError::Io(format!("F-Droid index request failed: {error}")))?;
            let bytes = response
                .bytes()
                .await
                .map_err(|error| AppError::Io(format!("Could not read F-Droid index: {error}")))?;
            let mut archive = ZipArchive::new(Cursor::new(bytes))
                .map_err(|error| AppError::Io(format!("Could not open F-Droid index: {error}")))?;
            let mut index_file = archive.by_name("index-v1.json").map_err(|error| {
                AppError::Io(format!("F-Droid index is missing index-v1.json: {error}"))
            })?;
            let mut index_json = String::new();
            index_file
                .read_to_string(&mut index_json)
                .map_err(|error| {
                    AppError::Io(format!("Could not read F-Droid index JSON: {error}"))
                })?;
            (
                select_fdroid_app_asset(&index_json, request.package_name, request.abi)?,
                None,
            )
        }
        ProvisionTermuxSource::Github => {
            let release_json = client
                .get(format!(
                    "https://api.github.com/repos/{}/releases/latest",
                    request.github_repository
                ))
                .send()
                .await
                .map_err(|error| AppError::Io(format!("Could not query Termux releases: {error}")))?
                .error_for_status()
                .map_err(|error| AppError::Io(format!("Termux release query failed: {error}")))?
                .text()
                .await
                .map_err(|error| {
                    AppError::Io(format!("Could not read Termux release metadata: {error}"))
                })?;
            let asset = select_github_asset_for_prefix(
                &release_json,
                request.github_asset_prefix,
                request.abi,
            )?;
            (asset, Some(release_json))
        }
    };
    if asset.sha256.is_none() {
        if let Some(url) = release_json.as_deref().and_then(checksum_asset_url) {
            match fetch_text_capped(&client, &url).await {
                Ok(text) => {
                    asset.sha256 = sha256_from_checksum_file(&text, &asset.file_name);
                }
                Err(error) => tracing::warn!(%error, "failed to use release checksum file"),
            }
        }
    }
    let mut response = client
        .get(&asset.url)
        .send()
        .await
        .map_err(|error| AppError::Io(format!("Could not download Termux APK: {error}")))?
        .error_for_status()
        .map_err(|error| AppError::Io(format!("Termux APK download failed: {error}")))?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_APK_BYTES as u64)
    {
        return Err(AppError::Config(format!(
            "Downloaded APK exceeds the {MAX_APK_MIB} MiB safety limit."
        )));
    }
    let total_bytes = response.content_length();
    let mut bytes = Vec::new();
    let mut last_reported_bytes = 0_u64;
    on_progress(0, total_bytes);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| AppError::Io(format!("Could not read Termux APK: {error}")))?
    {
        if chunk.len() > MAX_APK_BYTES.saturating_sub(bytes.len()) {
            return Err(AppError::Config(format!(
                "Downloaded APK exceeds the {MAX_APK_MIB} MiB safety limit."
            )));
        }
        bytes.extend_from_slice(&chunk);
        let downloaded_bytes = bytes.len() as u64;
        if should_report_download_progress(downloaded_bytes, last_reported_bytes, total_bytes) {
            on_progress(downloaded_bytes, total_bytes);
            last_reported_bytes = downloaded_bytes;
        }
    }
    let downloaded_bytes = bytes.len() as u64;
    if downloaded_bytes > last_reported_bytes {
        on_progress(downloaded_bytes, total_bytes);
    }
    let verified_against_source = asset.sha256.is_some();
    let sha256 = match asset.sha256.as_deref() {
        Some(expected) => {
            verify_sha256(&bytes, expected)?;
            expected.to_ascii_lowercase()
        }
        None => digest_hex(&bytes),
    };
    std::fs::create_dir_all(cache_dir)?;
    let path = cache_dir.join(format!("{sha256}-{}", request.cache_file_name));
    std::fs::write(&path, &bytes)?;
    Ok(DownloadedApk {
        path,
        file_name: asset.file_name,
        version_name: asset.version_name,
        version_code: asset.version_code,
        sha256,
        verified_against_source,
    })
}

fn should_report_download_progress(
    downloaded_bytes: u64,
    last_reported_bytes: u64,
    total_bytes: Option<u64>,
) -> bool {
    downloaded_bytes.saturating_sub(last_reported_bytes) >= DOWNLOAD_PROGRESS_INTERVAL_BYTES
        || total_bytes.is_some_and(|total| downloaded_bytes == total)
}

fn normalize_abi(abi: &str) -> AppResult<String> {
    let normalized = match abi {
        "arm64" | "arm64-v8a" => "arm64-v8a",
        "arm" | "armeabi-v7a" => "armeabi-v7a",
        "x86" | "x86_64" => abi,
        _ => return Err(AppError::Config(format!("Unsupported Android ABI: {abi}"))),
    };
    Ok(normalized.into())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

pub fn verify_sha256(bytes: &[u8], expected: &str) -> AppResult<()> {
    if !valid_sha256(expected) {
        return Err(AppError::Config(
            "APK metadata contains an invalid SHA-256 digest.".into(),
        ));
    }
    let actual = digest_hex(bytes);
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(AppError::Config(
            "Downloaded Termux APK SHA-256 does not match source metadata.".into(),
        ));
    }
    Ok(())
}

fn digest_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_interrupts_an_in_flight_download_operation() {
        let cancel = CancellationToken::new();
        cancel.cancel();

        let result = cancellable(&cancel, std::future::pending::<AppResult<()>>()).await;

        assert_eq!(result, Err(AppError::Cancelled));
    }

    const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn download_progress_is_throttled_but_reports_completion() {
        assert!(!should_report_download_progress(
            512 * 1024,
            0,
            Some(2 * 1024 * 1024)
        ));
        assert!(should_report_download_progress(
            1024 * 1024,
            0,
            Some(2 * 1024 * 1024)
        ));
        assert!(should_report_download_progress(1500, 1000, Some(1500)));
    }

    #[tokio::test]
    async fn receipt_identifies_only_matching_device_and_installed_apk() {
        let root = std::env::temp_dir().join(format!("hacc-receipt-test-{}", std::process::id()));
        let downloaded = DownloadedApk {
            path: root.join("termux.apk"),
            file_name: "termux.apk".into(),
            version_name: "0.118.3".into(),
            version_code: 1002,
            sha256: HASH.into(),
            verified_against_source: true,
        };
        record_termux_install(
            &root,
            "device-one",
            ProvisionTermuxSource::Fdroid,
            &downloaded,
        )
        .unwrap();
        let runner = crate::process::FakeRunner::new();
        runner.on(
            "-s phone shell path=$(pm path com.termux | head -n 1); sha256sum \"${path#package:}\"",
            Ok(crate::process::RawOutput::ok(format!(
                "{HASH}  /data/app/base.apk"
            ))),
        );
        let client = crate::adb::AdbClient::new("adb", std::sync::Arc::new(runner.clone()));
        let mut installed = crate::adb::TermuxPackageInfo {
            installed: true,
            version_name: Some("0.118.3".into()),
            version_code: Some(1002),
            installer: None,
            source: crate::adb::TermuxSource::Sideloaded,
            companions: Vec::new(),
        };
        apply_termux_install_receipt(&client, "phone", &root, "other-device", &mut installed)
            .await
            .unwrap();
        assert_eq!(installed.source, crate::adb::TermuxSource::Sideloaded);
        assert!(runner.calls().is_empty());
        apply_termux_install_receipt(&client, "phone", &root, "device-one", &mut installed)
            .await
            .unwrap();
        assert_eq!(installed.source, crate::adb::TermuxSource::FDroid);
        installed.source = crate::adb::TermuxSource::Sideloaded;
        let replacement_runner = crate::process::FakeRunner::new();
        replacement_runner.on(
            "-s phone shell path=$(pm path com.termux | head -n 1); sha256sum \"${path#package:}\"",
            Ok(crate::process::RawOutput::ok(format!(
                "{}  /data/app/base.apk",
                "b".repeat(64)
            ))),
        );
        let client = crate::adb::AdbClient::new("adb", std::sync::Arc::new(replacement_runner));
        apply_termux_install_receipt(&client, "phone", &root, "device-one", &mut installed)
            .await
            .unwrap();
        assert_eq!(installed.source, crate::adb::TermuxSource::Sideloaded);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selects_highest_matching_fdroid_apk_and_hash() {
        let index = serde_json::json!({
            "repo": { "name": "F-Droid" },
            "packages": {
                "com.other.app": [{ "apkName": "other.apk" }],
                "com.termux": [
                    {
                        "apkName": "com.termux_1001.apk",
                        "hash": HASH,
                        "hashType": "sha256",
                        "versionName": "0.118.2",
                        "versionCode": 1001,
                        "nativecode": ["armeabi-v7a"]
                    },
                    {
                        "apkName": "com.termux_1002.apk",
                        "hash": HASH,
                        "hashType": "sha256",
                        "versionName": "0.118.3",
                        "versionCode": 1002,
                        "nativecode": ["arm64-v8a"]
                    }
                ]
            }
        })
        .to_string();
        let asset = select_fdroid_asset(&index, "arm64-v8a").unwrap();
        assert_eq!(asset.file_name, "com.termux_1002.apk");
        assert_eq!(asset.version_code, 1002);
        assert_eq!(asset.url, "https://f-droid.org/repo/com.termux_1002.apk");
    }

    #[test]
    fn selects_github_arch_asset_and_requires_metadata_digest() {
        let release = serde_json::json!({
            "assets": [
                {
                    "name": "termux-app_v0.118.3+github-debug_universal.apk",
                    "browser_download_url": "https://github.invalid/universal.apk",
                    "digest": format!("sha256:{HASH}")
                },
                {
                    "name": "termux-app_v0.118.3+github-debug_arm64-v8a.apk",
                    "browser_download_url": "https://github.invalid/arm64.apk",
                    "digest": format!("sha256:{HASH}")
                }
            ]
        })
        .to_string();
        let asset = select_github_asset(&release, "arm64-v8a").unwrap();
        assert!(asset.file_name.contains("arm64-v8a"));
        assert_eq!(asset.sha256.as_deref(), Some(HASH));

        let no_digest = serde_json::json!({
            "assets": [
                {
                    "name": "termux-app_v0.118.3+github-debug_arm64-v8a.apk",
                    "browser_download_url": "https://github.invalid/arm64.apk"
                }
            ]
        })
        .to_string();
        let undigested = select_github_asset(&no_digest, "arm64-v8a").unwrap();
        assert!(undigested.sha256.is_none());

        let invalid_digest = serde_json::json!({
            "assets": [
                {
                    "name": "termux-app_v0.118.3+github-debug_arm64-v8a.apk",
                    "browser_download_url": "https://github.invalid/arm64.apk",
                    "digest": "sha256:not-a-digest"
                }
            ]
        })
        .to_string();
        assert!(select_github_asset(&invalid_digest, "arm64-v8a").is_err());

        let boot_release = release.replace("termux-app_", "termux-boot_");
        let boot =
            select_github_asset_for_prefix(&boot_release, "termux-boot_", "arm64-v8a").unwrap();
        assert!(boot.file_name.starts_with("termux-boot_"));
    }

    #[test]
    fn reads_sha256_from_release_checksum_file() {
        let apk_name = "termux-app_v0.118.3+github-debug_arm64-v8a.apk";
        let text = format!(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  other.apk\n{HASH}  {apk_name}\n"
        );
        assert_eq!(sha256_from_checksum_file(&text, apk_name).unwrap(), HASH);
        let starred = format!("{HASH} *{apk_name}\n");
        assert_eq!(sha256_from_checksum_file(&starred, apk_name).unwrap(), HASH);
        assert!(sha256_from_checksum_file(&text, "missing.apk").is_none());
        let invalid = format!(
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz  {apk_name}\n"
        );
        assert!(sha256_from_checksum_file(&invalid, apk_name).is_none());
    }

    #[test]
    fn finds_release_checksum_asset_but_never_other_apks() {
        let release = serde_json::json!({
            "assets": [
                { "name": "termux-app_v0.118.3_arm64-v8a.apk", "browser_download_url": "https://github.invalid/arm64.apk" },
                { "name": "SHA256SUMS.txt", "browser_download_url": "https://github.invalid/SHA256SUMS.txt" }
            ]
        })
        .to_string();
        assert_eq!(
            checksum_asset_url(&release).unwrap(),
            "https://github.invalid/SHA256SUMS.txt"
        );
        let no_checksums = serde_json::json!({
            "assets": [
                { "name": "termux-app_v0.118.3_arm64-v8a.apk", "browser_download_url": "https://github.invalid/arm64.apk" }
            ]
        })
        .to_string();
        assert!(checksum_asset_url(&no_checksums).is_none());
    }

    #[test]
    fn validates_abi_and_download_checksum() {
        assert_eq!(normalize_abi("arm64").unwrap(), "arm64-v8a");
        assert!(normalize_abi("mips").is_err());
        verify_sha256(b"termux", &digest_hex(b"termux")).unwrap();
        assert!(verify_sha256(b"tampered", HASH).is_err());
    }
}
