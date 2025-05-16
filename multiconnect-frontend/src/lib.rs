mod daemon;
mod modules;

use argh::FromArgs;
use daemon::Daemon;
use modules::{
  file_transfer::{self, FileTransferModule},
  pairing::{self, PairingModule},
  FrontendModuleManager,
};
use multiconnect_config::CONFIG;
use tauri::{async_runtime, Manager};
use tokio::task;

#[allow(dead_code)]
const PORT: u16 = 10999;

#[derive(FromArgs)]
#[argh(help_triggers("-h", "--help"))]
/// Sync devices
pub struct FrontendArgs {
  /// specify the port of the daemon to connect to (default 10999)
  #[argh(option, default = "PORT", short = 'p')]
  pub port: u16,
  /// specify the log level (default is info) {trace|debug|info|warn|error}
  #[argh(option, default = "String::from(\"info\")")]
  pub log_level: String,
}

pub fn run(port: u16) {
  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_opener::init())
    .setup(move |app| {
      let handle = app.handle();
      task::block_in_place(|| {
        async_runtime::block_on(async {
          let daemon = Daemon::connect(&port).await.unwrap();

          let mut manager = FrontendModuleManager::new(daemon, handle.clone());
          // Initialize modules
          manager.register(PairingModule::new());
          manager.register(FileTransferModule);

          manager.init().await;
          app.manage(manager);
        })
      });
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      set_theme,
      get_theme,
      pairing::send_pairing_request,
      pairing::refresh_devices,
      pairing::send_pairing_response,
      file_transfer::send_file
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

#[tauri::command]
async fn get_theme() -> Result<String, ()> {
  let cfg = CONFIG.get().unwrap();
  Ok(cfg.read().await.get_config().frontend.theme.clone())
}

#[tauri::command]
async fn set_theme(theme: String) -> Result<(), ()> {
  let cfg = CONFIG.get().unwrap();
  let mut cfg = cfg.write().await;
  cfg.get_mut_config().frontend.theme = theme;
  cfg.save_config().await.unwrap();
  Ok(())
}
