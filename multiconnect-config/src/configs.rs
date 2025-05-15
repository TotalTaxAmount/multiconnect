use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrontendConfig {
  pub theme: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModuleConfig {
  pub transfer_config: TransferConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransferConfig {
  pub save_path: String,
}

impl Default for TransferConfig {
  fn default() -> Self {
    Self { save_path: dirs::download_dir().unwrap().to_string_lossy().to_string() }
  }
}

impl Default for FrontendConfig {
  fn default() -> Self {
    Self { theme: "light".to_string() }
  }
}
