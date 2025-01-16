mod daemon;

use std::{error::Error, sync::Arc};

// use multiconnect_networking::{peer::Peer, NetworkManager};
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
  // let mut daemon = Daemon::new().await.unwrap();
  // daemon.ping().await;

  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    // .manage(network_manager)
    .invoke_handler(tauri::generate_handler![])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

// #[tauri::command]
// async fn list_peers(network_manager: State<'_, Arc<Mutex<NetworkManager>>>)
// -> Result<Vec<Peer>, ()> {   let manager = network_manager.lock().await;
//   Ok(manager.list_peers().await)
// }
