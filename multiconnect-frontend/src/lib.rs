mod controller;
mod daemon;

use std::str::FromStr;

use argh::FromArgs;
use controller::Controller;
use daemon::Daemon;
use multiconnect_config::CONFIG;
use multiconnect_protocol::{local::peer::*, Device, Packet};
use tauri::{async_runtime, Manager, State};
use tokio::task;
use uuid::Uuid;

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
    .plugin(tauri_plugin_opener::init())
    .setup(move |app| {
      let handle = app.handle();
      task::block_in_place(|| {
        async_runtime::block_on(async {
          let daemon = Daemon::connect(&port).await.unwrap();
          let controller = Controller::new(daemon, handle.clone()).await;
          app.manage(controller)
        })
      });
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      send_pairing_request,
      send_pairing_response,
      refresh_mdns,
      set_theme,
      get_theme
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

/// Send a pairing request to a device
/// Parameters:
/// * `device` - The device to send the request to
#[tauri::command]
async fn send_pairing_request(controller: State<'_, Controller>, device: Device) -> Result<(), ()> {
  let uuid = Uuid::new_v4();
  controller.send_packet(Packet::L2PeerPairRequest(L2PeerPairRequest::new(&device, uuid))).await;
  Ok(())
}

/// Send a response to a pairing request
/// Parameters:
/// * `accepted` - If the pairing request is accepted
/// * `req_uuid` - The uuid of the request
#[tauri::command]
async fn send_pairing_response(controller: State<'_, Controller>, accepted: bool, req_uuid: &str) -> Result<(), ()> {
  controller
    .send_packet(Packet::L3PeerPairResponse(L3PeerPairResponse::new(accepted, Uuid::from_str(req_uuid).unwrap())))
    .await;
  Ok(())
}

/// Refresh mDNS
#[tauri::command]
async fn refresh_mdns(controller: State<'_, Controller>) -> Result<(), ()> {
  controller.send_packet(Packet::L4Refresh(L4Refresh::new())).await;
  Ok(())
}

#[tauri::command]
async fn get_theme() -> Result<String, ()> {
  Ok(CONFIG.read().await.get_config().frontend.theme.clone())
}

#[tauri::command]
async fn set_theme(theme: String) -> Result<(), ()> {
  let mut cfg = CONFIG.write().await;
  cfg.get_mut_config().frontend.theme = theme;
  cfg.save_config();
  Ok(())
}
