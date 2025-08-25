mod configs;

use configs::{FrontendConfig, ModuleConfig};
use fs2::FileExt;
use log::{debug, error, info};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::{
  fs::{self, File, OpenOptions},
  path::{Path, PathBuf},
  process::exit,
  str::FromStr,
};
use thiserror::Error;
use tokio::sync::RwLock;

pub static CONFIG: OnceCell<RwLock<ConfigManager>> = OnceCell::new();

#[derive(Debug)]
pub struct ConfigManager {
  config_path: PathBuf,
  config: Config,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Config {
  #[serde(default)]
  pub frontend: FrontendConfig,
  #[serde(default)]
  pub modules: ModuleConfig,
}

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error("I/O error: {0}")]
  Io(#[from] std::io::Error),

  #[error("Serialization error: {0}")]
  Serialize(#[from] serde_yml::Error),

  #[error("Config directory could not be determined")]
  ConfigDirNotFound,
}

impl ConfigManager {
  pub async fn new() -> Result<Self, ConfigError> {
    let path = if let Ok(other) = std::env::var("MC_CONFIG_DIR") {
      PathBuf::from_str(&other).unwrap() // safe: env var is UTF-8
    } else {
      dirs::config_dir().ok_or(ConfigError::ConfigDirNotFound)?.join("multiconnect")
    };

    let config_file_path = path.join("config.yml");

    if !path.exists() {
      fs::create_dir_all(&path)?;
    }

    if !config_file_path.exists() {
      info!("Creating new config file: {}", config_file_path.to_string_lossy());
      Self::create_default_config(&config_file_path)?;
    }

    let config = Self::load_config(config_file_path).await?;

    Ok(Self { config_path: path, config })
  }

  pub async fn init() {
    match ConfigManager::new().await {
      Ok(cfg) => {
        CONFIG.set(RwLock::new(cfg)).unwrap();
      }
      Err(e) => {
        error!("Failed to initialize config manager: {}", e);
        exit(1);
      }
    }
  }

  pub fn get_config_dir(&self) -> &PathBuf {
    &self.config_path
  }

  fn create_default_config(path: &Path) -> Result<(), ConfigError> {
    let config = Config::default();

    let file = OpenOptions::new().write(true).truncate(true).create(true).open(path)?;

    file.lock_exclusive()?;
    serde_yml::to_writer(&file, &config)?;
    file.unlock()?;

    Ok(())
  }

  async fn load_config(path: PathBuf) -> Result<Config, ConfigError> {
    tokio::task::spawn_blocking(move || {
      if !path.exists() {
        let default = Config::default();
        let f = OpenOptions::new().write(true).truncate(true).create(true).open(&path)?;
        serde_yml::to_writer(&f, &default)?;
        return Ok(default);
      }

      let file = File::open(&path)?;
      file.lock_exclusive()?;

      let mut config: Config = serde_yml::from_reader(&file)?;

      file.unlock()?;

      // Re-serialize to normalize formatting
      let serialized = serde_yml::to_string(&config)?;
      config = serde_yml::from_str(&serialized)?;

      let f = OpenOptions::new().write(true).truncate(true).create(true).open(&path)?;
      serde_yml::to_writer(&f, &config)?;

      Ok(config)
    })
    .await
    .map_err(|e| ConfigError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
  }

  pub async fn save_config(&self) -> Result<(), ConfigError> {
    let config_path = self.config_path.join("config.yml");
    let config = self.config.clone();

    debug!("Saving config file");

    tokio::task::spawn_blocking(move || {
      let file = OpenOptions::new().write(true).truncate(true).create(true).open(&config_path)?;
      file.lock_exclusive()?;
      serde_yml::to_writer(&file, &config)?;
      file.unlock()?;
      Ok::<(), ConfigError>(())
    })
    .await
    .map_err(|e| ConfigError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))??;

    Ok(())
  }

  pub fn get_config(&self) -> &Config {
    &self.config
  }

  pub fn get_mut_config(&mut self) -> &mut Config {
    &mut self.config
  }
}
