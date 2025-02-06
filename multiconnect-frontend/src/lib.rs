mod daemon;

use std::sync::Arc;

use daemon::{Daemon, DaemonController};
use log::info;
use multiconnect_protocol::{Peer, Packet};
use tauri::State;
use tokio::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub async fn run() {
  if std::env::var("MULTICONNECT_LOG").is_err() {
    std::env::set_var("MULTICONNECT_LOG", "info");
  }

  pretty_env_logger::formatted_timed_builder().parse_env("MULTICONNECT_LOG").format_timestamp_secs().init();

  // let network_manager: Arc<Mutex<NetworkManager>> =
  // NetworkManager::new().await.unwrap();
  let daemon = Daemon::connect().await.unwrap();
  let controller = DaemonController::bind(daemon).await;

  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .manage(controller)
    .invoke_handler(tauri::generate_handler![list_peers])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

#[tauri::command]
async fn list_peers(controller: State<'_, DaemonController>) -> Result<Vec<Peer>, ()> {
  Ok(controller.get_peers().await)
}
