use log::warn;
use serde::{de, Deserialize, Serialize};

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
  /// Configuration for the file transfer module
  #[serde(default)]
  pub transfer: TransferConfig,
}

impl Default for ModuleConfig {
  fn default() -> Self {
    Self { transfer: TransferConfig::default() }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferConfig {
  /// The path where files will be saved
  /// Defaults to the user's download directory
  #[serde(default = "default_save_path")]
  pub save_path: String,
  /// The maximum bytes per second to transfer
  /// Defaults to 10 MB/s
  #[serde(default = "default_max_bps")]
  pub max_bps: usize,
}

fn default_save_path() -> String {
  dirs::download_dir().map(|d| d.to_string_lossy().to_string()).unwrap_or_else(|| {
    warn!("Could not find download dir, defaulting to '.'");
    ".".into()
  })
}

fn default_max_bps() -> usize {
  1024 * 1024 * 100
}

impl Default for TransferConfig {
  fn default() -> Self {
    Self { save_path: default_save_path(), max_bps: default_max_bps() }
  }
}
