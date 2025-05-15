use log::warn;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
  #[serde(default)]
  pub frontend: FrontendConfig,

  #[serde(default)]
  pub modules: ModuleConfig,
}

impl Default for Config {
  fn default() -> Self {
    Self { frontend: FrontendConfig::default(), modules: ModuleConfig::default() }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendConfig {
  #[serde(default = "default_theme")]
  pub theme: String,
}

fn default_theme() -> String {
  "light".into()
}

impl Default for FrontendConfig {
  fn default() -> Self {
    Self { theme: default_theme() }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleConfig {
  #[serde(default)]
  pub transfer_config: TransferConfig,
}

impl Default for ModuleConfig {
  fn default() -> Self {
    Self { transfer_config: TransferConfig::default() }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferConfig {
  #[serde(default)]
  pub save_path: String,
}

impl Default for TransferConfig {
  fn default() -> Self {
    let save_path = dirs::download_dir().map(|d| d.to_string_lossy().to_string()).unwrap_or_else(|| {
      warn!("Could not find download dir, defaulting to '.'");
      ".".into()
    });
    Self { save_path }
  }
}
