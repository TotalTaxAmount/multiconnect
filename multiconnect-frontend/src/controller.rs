use log::warn;
use multiconnect_protocol::{Device, Packet};
use tauri::{AppHandle, Emitter};

use crate::daemon::SharedDaemon;

#[derive(Debug)]
pub struct Controller {
  daemon: SharedDaemon,
}

impl Controller {
  pub async fn new(daemon: SharedDaemon, app: AppHandle) -> Self {
    let daemon_clone = daemon.clone();
    tokio::spawn(async move {
      let mut stream = daemon_clone.packet_stream();
      loop {
        // let mut locked = daemon_clone.lock().await;
        if let Ok(res) = stream.recv().await {
          match res {
            Packet::L0PeerFound(packet) => {
              let _ = app.emit("peer-found", bincode::deserialize::<Device>(&packet.device).unwrap());
            }
            Packet::L1PeerExpired(packet) => {
              let _ = app.emit("peer-expired", packet.peer_id);
            }
            Packet::L2PeerPairRequest(packet) => {
              let _ = app.emit(
                "pair-request",
                (&packet.req_uuid.to_string(), bincode::deserialize::<Device>(&packet.device).unwrap()),
              );
            }
            _ => {}
          }
        } else {
          warn!("Stream closed");
          break;
        }
      }
    });

    Self { daemon }
  }

  /// Send a packet to the daemon
  pub async fn send_packet(&self, packet: Packet) {
    self.daemon.send_packet(packet).await;
  }
}
