use log::warn;
use multiconnect_protocol::Packet;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::daemon::SharedDaemon;

#[derive(Debug)]
pub struct Controller {
  daemon: SharedDaemon,
  app: AppHandle
}

impl Controller {
  pub fn new(daemon: SharedDaemon, app: AppHandle) -> Self {
    Self { daemon, app }
  }

  pub async fn add_to_queue(&mut self, packet: Packet) {
    self.daemon.lock().await.add_to_queue(packet).await;
  }

  pub async fn on_packet(&mut self) -> Option<Packet> {
    self.daemon.lock().await.on_packet().await
  }

  pub fn emit<T: Serialize + Clone>(&self, event: &str, payload: T) {
    if let Err(e) = self.app.emit(event, payload) {
      warn!("Failed to emit event")
    }
  }
}