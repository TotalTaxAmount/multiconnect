mod daemon;
mod controller;

use std::sync::Arc;

use daemon::Daemon;
use controller::Controller;
use log::info;
use multiconnect_protocol::{Peer, Packet};
use tauri::{async_runtime, App, Manager, State};
use tokio::{sync::Mutex, task};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  if std::env::var("MULTICONNECT_LOG").is_err() {
    std::env::set_var("MULTICONNECT_LOG", "info");
  }

  pretty_env_logger::formatted_timed_builder().parse_env("MULTICONNECT_LOG").format_timestamp_secs().init();

  // let daemon = Daemon::connect().await.unwrap();


  tauri::Builder::default()
  .plugin(tauri_plugin_opener::init())
  .setup(|app| {
    let handle = app.handle();
    task::block_in_place(|| {
      async_runtime::block_on(async {
        let daemon = Daemon::connect().await.unwrap();
        let controller = Controller::new(daemon, handle.clone());
        app.manage(controller)
      })
    });
    Ok(())
  })
  .invoke_handler(tauri::generate_handler![])
  .run(tauri::generate_context!())
  .expect("error while running tauri application");
  

  

}
  
// #[tauri::command]
// async fn list_peers(controller: State<'_, Controller>) -> Result<Vec<Peer>, ()> {
//   Ok(controller.get_peers().await)
// }
