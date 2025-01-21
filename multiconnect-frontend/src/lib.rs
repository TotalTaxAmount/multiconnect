mod daemon;

use std::{sync::Arc, time::Duration};

use daemon::Daemon;
use tokio::{sync::Mutex, time::interval};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub async fn run() {
  if std::env::var("MULTICONNECT_LOG").is_err() {
    std::env::set_var("MULTICONNECT_LOG", "info");
  }

  pretty_env_logger::formatted_timed_builder().parse_env("MULTICONNECT_LOG").format_timestamp_secs().init();

  // let network_manager: Arc<Mutex<NetworkManager>> =
  // NetworkManager::new().await.unwrap();
  let daemon = Daemon::new().await.unwrap();
  monitor_daemon(daemon.clone()).await;

  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    // .manage(network_manager)
    .invoke_handler(tauri::generate_handler![])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

async fn monitor_daemon(daemon: Arc<Mutex<Daemon>>) {
  tokio::spawn(async move {
    let mut interval = interval(Duration::from_secs(3));

    loop {
      interval.tick().await;
      daemon.lock().await.ping().await;
    }
  });
}
// #[tauri::command]
// async fn list_peers(network_manager: State<'_, Arc<Mutex<NetworkManager>>>)
// -> Result<Vec<Peer>, ()> {   let manager = network_manager.lock().await;
//   Ok(manager.list_peers().await)
// }
