use log::warn;
use multiconnect_protocol::{Packet, Peer};
use serde::Serialize;
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
      loop {
        let mut locked = daemon_clone.lock().await;
        match locked.on_packet().await {
          Some(Packet::PeerFound(packet)) => {
            let _ = app.emit("peer-found", bincode::deserialize::<Peer>(&packet.peer).unwrap());
          }
          Some(Packet::PeerExpired(packet)) => {
            let _ = app.emit("peer-expired", bincode::deserialize::<Peer>(&packet.peer).unwrap());
          }
          Some(_) | None => {}
        }
      }
    });

    Self { daemon }
  }

  pub async fn add_to_queue(&mut self, packet: Packet) {
    self.daemon.lock().await.add_to_queue(packet).await;
  }
}