use std::{
  error::Error,
  fs::{self, File, OpenOptions},
  io::Write,
  path::PathBuf,
  process::exit,
  str::{self, FromStr},
};

use lazy_static::lazy_static;
use log::{error, info};
use serde::{Deserialize, Serialize};

lazy_static! {
  pub static ref CONFIG: ConfigManager = ConfigManager::new();
}

pub struct ConfigManager {
  config_path: PathBuf,
  config: Config,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Config {
  x: u8,
}

impl ConfigManager {
  fn new() -> Self {
    let path: PathBuf;

    if let Ok(other) = std::env::var("MC_CONFIG_DIR") {
      path = PathBuf::from_str(&other).unwrap();
    } else {
      path = dirs::config_dir().unwrap().join("multiconnect");
    }

    let mut config_file_path = path.clone();
    config_file_path.push("config.yml");

    if !path.exists() {
      if let Err(e) = fs::create_dir_all(&path) {
        error!("Failed to create config dir: {}", e);
        exit(1);
      }
    }

    if !config_file_path.exists() {
      info!("Creating new config file: {}", config_file_path.to_str().unwrap());
      Self::create_default_config(&config_file_path);
    }

    if let Ok(config) = Self::load_config(&config_file_path) {
      Self { config_path: path, config }
    } else {
      error!("Failed to load config");
      exit(-1)
    }
  }

  pub fn get_config_dir(&self) -> &PathBuf {
    &self.config_path
  }

  fn create_default_config(path: &PathBuf) {
    let config = Config::default();

    let file = OpenOptions::new()
      .write(true)
      .truncate(true)
      .create(true)
      .open(path)
      .expect(&format!("Failed to open config file: {:?}", path));

    serde_yml::to_writer(file, &config).unwrap();
  }

  fn load_config(path: &PathBuf) -> Result<Config, Box<dyn Error>> {
    let file = File::open(path)?;
    let config: Config = serde_yml::from_reader(file)?;
    Ok(config)
  }
}
