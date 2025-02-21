mod controller;
mod daemon;

use controller::Controller;
use daemon::Daemon;
use tauri::{async_runtime, Manager};
use tokio::task;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  if std::env::var("MULTICONNECT_LOG").is_err() {
    std::env::set_var("MULTICONNECT_LOG", "info");
  }

  pretty_env_logger::formatted_timed_builder().parse_env("MULTICONNECT_LOG").format_timestamp_secs().init();

  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .setup(|app| {
      let handle = app.handle();
      task::block_in_place(|| {
        async_runtime::block_on(async {
          let daemon = Daemon::connect().await.unwrap();
          let controller = Controller::new(daemon, handle.clone()).await;
          app.manage(controller)
        })
      });
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

#[tauri::command]
async fn send_pairing_request(controller: State<'_, Controller>) -> Result<(),()> {   
  
}
