use std::{
  collections::HashMap,
  fs::{remove_file, File, OpenOptions},
  io::{Read, Write},
};

use libp2p::PeerId;
use log::error;
use multiconnect_config::CONFIG;
use multiconnect_protocol::Device;

const FILENAME: &str = "saved_devices.json";

pub struct Store {
  saved_devices: HashMap<PeerId, (Device, bool)>,
}

impl Store {
  pub async fn new() -> Self {
    Self { saved_devices: Self::load().await }
  }

  async fn load() -> HashMap<PeerId, (Device, bool)> {
    let cfg = CONFIG.get().unwrap();
    let path = cfg.read().await.get_config_dir().join(FILENAME);
    if !path.exists() {
      return HashMap::new();
    }

    let mut file = File::open(&path).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    let raw = String::from_utf8(buf).unwrap();

    if let Ok(saved_devices) = serde_json::from_str(&raw) {
      return saved_devices;
    } else {
      error!("Failed to read saved saved_devices from file");
      let _ = remove_file(path);
      return HashMap::new();
    }
  }

  async fn save(&self) {
    let cfg = CONFIG.get().unwrap();
    let file = cfg.read().await.get_config_dir().join(FILENAME);
    let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(file).unwrap();

    let json = serde_json::to_string(&self.saved_devices).unwrap();
    file.write_all(&json.as_bytes()).unwrap();
  }

  pub async fn save_device(&mut self, peer_id: PeerId, device: Device, paired: bool) {
    self.saved_devices.insert(peer_id, (device, paired));
    self.save().await;
  }

  pub fn get_saved_devices(&self) -> &HashMap<PeerId, (Device, bool)> {
    &self.saved_devices
  }

  pub fn check_paired(&self, peer_id: PeerId) -> Option<&(Device, bool)> {
    self.saved_devices.get(&peer_id)
  }

  pub async fn remove_device(&mut self, peer_id: PeerId) -> Option<(Device, bool)> {
    let paired = self.saved_devices.remove(&peer_id);
    self.save().await;
    paired
  }

  pub async fn set_paired(&mut self, peer_id: PeerId, paired: bool) {
    if let Some((_, p)) = self.saved_devices.get_mut(&peer_id) {
      *p = paired;
    }
  }
}
