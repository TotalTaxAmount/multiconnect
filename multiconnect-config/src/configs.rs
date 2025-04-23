use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrontendConfig {
  pub theme: String,
}

impl Default for FrontendConfig {
  fn default() -> Self {
    Self { theme: "light".to_string() }
  }
}
