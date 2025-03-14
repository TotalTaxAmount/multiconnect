mod controller;
mod daemon;

use controller::Controller;
use daemon::Daemon;
use multiconnect_protocol::{
  local::peer::{L2PeerPairRequest, L3PeerPairResponse},
  Packet, Peer,
};
use tauri::{async_runtime, Manager, State};
use tokio::task;

pub fn run() {
  if std::env::var("MC_LOG").is_err() {
    std::env::set_var("MC_LOG", "info");
  }

  pretty_env_logger::formatted_timed_builder().parse_env("MC_LOG").format_timestamp_secs().init();

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
    .invoke_handler(tauri::generate_handler![send_pairing_request, send_pairing_response])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

#[tauri::command]
async fn send_pairing_request(controller: State<'_, Controller>, peer: Peer) -> Result<(), ()> {
  controller.send_packet(Packet::L2PeerPairRequest(L2PeerPairRequest::new(&peer.peer_id))).await;
  Ok(())
}

#[tauri::command]
async fn send_pairing_response(controller: State<'_, Controller>, accepted: bool, req_id: u32) -> Result<(), ()> {
  controller.send_packet(Packet::L3PeerPairResponse(L3PeerPairResponse::new(accepted, req_id))).await;
  Ok(())
}
