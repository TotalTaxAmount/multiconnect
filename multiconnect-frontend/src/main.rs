// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{error::Error, str::FromStr, time::SystemTime};

use fern::colors::{Color, ColoredLevelConfig};
use multiconnect_config::ConfigManager;
use multiconnect_lib::FrontendArgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let args: FrontendArgs = argh::from_env();

  multiconnect_core::init_tracing(&args.log_level, args.port + 2)?; // Daemon on +1

  ConfigManager::init().await;

  Ok(multiconnect_lib::run(args.port))
}
