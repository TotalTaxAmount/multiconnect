use log::warn;
use multiconnect_protocol::{Packet, Peer};
use tauri::{AppHandle, Emitter};
use tokio_stream::StreamExt;

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
        if let Some(res) = stream.next().await {
          match res {
            Ok(Packet::PeerFound(packet)) => {
              let _ = app.emit("peer-found", bincode::deserialize::<Peer>(&packet.peer).unwrap());
            }
            Ok(Packet::PeerExpired(packet)) => {
              let _ = app.emit("peer-expired", packet.peer_id);
            }
            Ok(Packet::PeerPairRequest(packet)) => {
              let _ = app.emit("pair-request", bincode::deserialize::<Peer>(&packet.peer).unwrap());
            }
            Ok(_) | Err(_) => {}
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
