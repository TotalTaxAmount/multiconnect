mod configs;

use std::{
  error::Error,
  fs::{self, File, OpenOptions},
  path::PathBuf,
  process::exit,
  str::{self, FromStr},
};

use configs::{FrontendConfig, ModuleConfig};
use fs2::FileExt;
use log::{debug, error, info};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
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

impl ConfigManager {
  async fn new() -> std::io::Result<Self> {
    let path: PathBuf;

    if let Ok(other) = std::env::var("MC_CONFIG_DIR") {
      path = PathBuf::from_str(&other).unwrap();
    } else {
      path = dirs::config_dir().unwrap().join("multiconnect");
    }

    let config_file_path = path.clone().join("config.yml");

    if !path.exists() {
      if let Err(e) = fs::create_dir_all(&path) {
        error!("Failed to create config dir: {}", e);
        exit(1);
      }
    }

    if !config_file_path.exists() {
      info!("Creating new config file: {}", config_file_path.to_str().unwrap());
      Self::create_default_config(&config_file_path)?;
    }

    if let Ok(config) = Self::load_config(config_file_path).await {
      Ok(Self { config_path: path, config })
    } else {
      error!("Failed to load config");
      exit(-1)
    }
  }

  pub async fn init() {
    let cfg = ConfigManager::new().await.expect("Failed to initalize config manager");
    CONFIG.set(RwLock::new(cfg)).unwrap();
  }

  pub fn get_config_dir(&self) -> &PathBuf {
    &self.config_path
  }

  fn create_default_config(path: &PathBuf) -> std::io::Result<()> {
    let config = Config::default();

    let file = OpenOptions::new()
      .write(true)
      .truncate(true)
      .create(true)
      .open(path)
      .expect(&format!("Failed to open config file: {:?}", path));
    file.lock_exclusive()?;
    serde_yml::to_writer(&file, &config).unwrap();
    fs2::FileExt::unlock(&file)?;

    Ok(())
  }

  async fn load_config(path: PathBuf) -> Result<Config, Box<dyn Error + Send>> {
    tokio::task::spawn_blocking(move || {
      // If no config file exists, create one with defaults
      if !path.exists() {
        let default = Config::default();
        let f = OpenOptions::new().write(true).truncate(true).create(true).open(&path).unwrap();
        serde_yml::to_writer(&f, &default).unwrap();
        return Ok(default);
      }

      let file = File::open(&path).unwrap();
      file.lock_exclusive().unwrap();

      let mut config: Config = serde_yml::from_reader(&file).unwrap();

      file.unlock().unwrap();

      let serialized = serde_yml::to_string(&config).unwrap();
      config = serde_yml::from_str(&serialized).unwrap();

      let f = OpenOptions::new().write(true).truncate(true).create(true).open(&path).unwrap();
      serde_yml::to_writer(&f, &config).unwrap();

      Ok(config)
    })
    .await
    .unwrap()
  }

  pub async fn save_config(&self) -> std::io::Result<()> {
    let config_path = self.config_path.clone().join("config.yml");
    let config = self.config.clone();

    debug!("Saving config file");

    tokio::task::spawn_blocking(move || {
      match OpenOptions::new().write(true).truncate(true).create(true).open(&config_path) {
        Ok(file) => {
          if let Err(e) = file.lock_exclusive() {
            error!("Failed to lock config file: {}", e);
            return;
          }

          if let Err(e) = serde_yml::to_writer(&file, &config) {
            error!("Failed to write config: {}", e);
          }

          if let Err(e) = fs2::FileExt::unlock(&file) {
            error!("Failed to unlock config file: {}", e);
          }
        }
        Err(e) => {
          error!("Failed to open config for saving: {}", e);
        }
      }
    })
    .await
    .unwrap();

    Ok(())
  }

  pub fn get_config(&self) -> &Config {
    &self.config
  }

  pub fn get_mut_config(&mut self) -> &mut Config {
    &mut self.config
  }
}
