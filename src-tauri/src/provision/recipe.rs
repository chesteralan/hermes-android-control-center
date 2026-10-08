use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::config::StartMode;
use crate::provision::{DEFAULT_MINIMUM_FREE_GIB, GIB_BYTES};

const DEBIAN_RECIPE: &str = include_str!("../../recipes/debian-official.toml");
const TERMUX_NATIVE_RECIPE: &str = include_str!("../../recipes/termux-native-apt.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ProvisionTermuxSource {
    #[serde(rename = "fdroid")]
    #[ts(rename = "fdroid")]
    Fdroid,
    #[serde(rename = "github")]
    #[ts(rename = "github")]
    Github,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export)]
pub struct HermesInstallRecipe {
    #[serde(default)]
    pub script_url: Option<String>,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub native_apt_package: Option<String>,
    #[serde(default)]
    pub apt_key_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export)]
pub struct HermesConfigureRecipe {
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export)]
pub struct HermesRuntimeRecipe {
    pub start_mode: StartMode,
    pub gateway_command: String,
    pub process_match: String,
    pub status_commands: Vec<String>,
    pub version_command: String,
    pub doctor_command: String,
    pub update_command: String,
    pub log_files: Vec<String>,
    pub state_file: String,
    #[serde(default)]
    pub path_prepend: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export)]
pub struct ProvisionRecipe {
    pub id: String,
    pub name: String,
    pub termux_source: ProvisionTermuxSource,
    pub distro: String,
    #[serde(default = "default_minimum_free_gib")]
    pub minimum_free_gib: u32,
    #[serde(default)]
    pub minimum_termux_version: Option<String>,
    pub termux_packages: Vec<String>,
    pub distro_packages: Vec<String>,
    pub hermes_install: HermesInstallRecipe,
    pub hermes_configure: HermesConfigureRecipe,
    pub hermes_runtime: HermesRuntimeRecipe,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub experimental: bool,
}

impl ProvisionRecipe {
    pub fn minimum_free_bytes(&self) -> u64 {
        u64::from(self.minimum_free_gib) * GIB_BYTES
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        let value: toml::Value = toml::from_str(source).map_err(|error| error.to_string())?;
        let json = serde_json::to_value(camelize_toml(value)).map_err(|error| error.to_string())?;
        let recipe: Self = serde_json::from_value(json).map_err(|error| error.to_string())?;
        recipe.validate()?;
        Ok(recipe)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || !self.id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err(
                "Recipe id may contain only letters, numbers, hyphens, and underscores.".into(),
            );
        }
        if self.name.trim().is_empty() {
            return Err("Recipe name is required.".into());
        }
        if self.minimum_free_gib == 0 {
            return Err("Minimum free storage must be at least 1 GiB.".into());
        }
        if !self.distro.is_empty()
            && !self.distro.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err(
                "Distro name may contain only letters, numbers, hyphens, and underscores.".into(),
            );
        }
        for package in self.termux_packages.iter().chain(&self.distro_packages) {
            if package.is_empty()
                || !package.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '+' | '-')
                })
            {
                return Err(format!("Invalid package name: {package:?}"));
            }
        }
        if self.termux_packages.is_empty() {
            return Err("At least one Termux package is required.".into());
        }
        let has_native_package = self.hermes_install.native_apt_package.is_some();
        match (
            has_native_package,
            self.hermes_install.script_url.as_deref(),
        ) {
            (false, Some(url)) if url.starts_with("https://") => {}
            (false, _) => return Err("Hermes installer script URL must use HTTPS.".into()),
            (true, _) => {
                if !self.distro.is_empty() {
                    return Err("Native Hermes recipes must not select a proot distro.".into());
                }
                let fingerprint = self
                    .hermes_install
                    .apt_key_fingerprint
                    .as_deref()
                    .ok_or("Native Hermes recipes require an APT signing-key fingerprint.")?;
                let normalized = fingerprint.replace(' ', "");
                if normalized.len() != 40 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(
                        "APT signing-key fingerprint must contain 40 hexadecimal characters."
                            .into(),
                    );
                }
            }
        }
        if !has_native_package && self.distro.is_empty() {
            return Err("A proot recipe must select a distro.".into());
        }
        if self.hermes_configure.steps.is_empty()
            || self
                .hermes_configure
                .steps
                .iter()
                .any(|step| step.trim().is_empty())
        {
            return Err("Hermes configure steps must be non-empty commands.".into());
        }
        let runtime = &self.hermes_runtime;
        if runtime.gateway_command.trim().is_empty()
            || runtime.process_match.trim().is_empty()
            || runtime.version_command.trim().is_empty()
            || runtime.doctor_command.trim().is_empty()
            || runtime.update_command.trim().is_empty()
            || runtime.status_commands.is_empty()
            || runtime.log_files.is_empty()
            || runtime.state_file.trim().is_empty()
        {
            return Err("Hermes runtime commands and paths must be configured.".into());
        }
        Ok(())
    }
}

fn default_minimum_free_gib() -> u32 {
    DEFAULT_MINIMUM_FREE_GIB
}

fn snake_to_camel(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut uppercase_next = false;
    for character in value.chars() {
        if character == '_' {
            uppercase_next = true;
        } else if uppercase_next {
            output.extend(character.to_uppercase());
            uppercase_next = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn camelize_toml(value: toml::Value) -> toml::Value {
    match value {
        toml::Value::Table(table) => toml::Value::Table(
            table
                .into_iter()
                .map(|(key, value)| (snake_to_camel(&key), camelize_toml(value)))
                .collect(),
        ),
        toml::Value::Array(values) => {
            toml::Value::Array(values.into_iter().map(camelize_toml).collect())
        }
        other => other,
    }
}

pub fn bundled_recipes() -> Result<Vec<ProvisionRecipe>, String> {
    [DEBIAN_RECIPE, TERMUX_NATIVE_RECIPE]
        .into_iter()
        .map(ProvisionRecipe::parse)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_recipes_parse_and_validate() {
        let recipes = bundled_recipes().unwrap();
        assert_eq!(recipes.len(), 2);
        assert_eq!(recipes[0].id, "debian-official");
        assert_eq!(recipes[0].termux_source, ProvisionTermuxSource::Fdroid);
        assert_eq!(recipes[0].distro, "debian");
        assert_eq!(recipes[1].id, "termux-native-apt");
        assert!(recipes[1].experimental);
        assert_eq!(
            recipes[1].hermes_install.apt_key_fingerprint.as_deref(),
            Some("C572 B5FD D1A2 9CCF A9A9 12B6 840B 0848 E139 156D")
        );
    }

    #[test]
    fn recipe_round_trips_through_toml() {
        let recipe = bundled_recipes().unwrap().remove(0);
        let encoded = toml::to_string(&recipe).unwrap();
        assert_eq!(ProvisionRecipe::parse(&encoded).unwrap(), recipe);
    }

    #[test]
    fn legacy_recipe_defaults_minimum_free_storage_to_two_gib() {
        let source = DEBIAN_RECIPE.replace("minimum_free_gib = 2\n", "");
        let recipe = ProvisionRecipe::parse(&source).unwrap();

        assert_eq!(recipe.minimum_free_gib, DEFAULT_MINIMUM_FREE_GIB);
        assert_eq!(recipe.minimum_free_bytes(), 2 * GIB_BYTES);
    }

    #[test]
    fn rejects_zero_minimum_free_storage() {
        let mut recipe = bundled_recipes().unwrap().remove(0);
        recipe.minimum_free_gib = 0;

        assert!(recipe.validate().unwrap_err().contains("at least 1 GiB"));
    }

    #[test]
    fn rejects_unsafe_distro_packages_and_non_https_installers() {
        let mut recipe = bundled_recipes().unwrap().remove(0);
        recipe.distro = "debian;id".into();
        assert!(recipe.validate().unwrap_err().contains("Distro name"));

        let mut recipe = bundled_recipes().unwrap().remove(0);
        recipe.distro_packages.push("curl;id".into());
        assert!(recipe.validate().unwrap_err().contains("package name"));

        let mut recipe = bundled_recipes().unwrap().remove(0);
        recipe.hermes_install.script_url = Some("http://example.invalid/install.sh".into());
        assert!(recipe.validate().unwrap_err().contains("HTTPS"));
    }

    #[test]
    fn rejects_native_recipe_without_valid_signing_fingerprint() {
        let mut recipe = bundled_recipes().unwrap().remove(1);
        recipe.hermes_install.apt_key_fingerprint = Some("ABCD".into());
        assert!(recipe.validate().unwrap_err().contains("40 hexadecimal"));
    }
}
