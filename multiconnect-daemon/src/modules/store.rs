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
  devices: HashMap<PeerId, (Device, Option<bool>)>,
}

impl Store {
  pub async fn new() -> Self {
    Self { devices: Self::load().await }
  }

  async fn load() -> HashMap<PeerId, (Device, Option<bool>)> {
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

  pub async fn save(&self) {
    let cfg = CONFIG.get().unwrap();
    let file = cfg.read().await.get_config_dir().join(FILENAME);
    let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(file).unwrap();

    let filtered: HashMap<_, _> = self
      .devices
      .iter()
      .filter_map(|(k, (d, p))| {
        if p.is_some() {
          Some((k.clone(), (d.clone(), p.clone())))
        } else {
          None
        }
      })
      .collect();

    let json = serde_json::to_string(&filtered).unwrap();
    file.write_all(&json.as_bytes()).unwrap();
  }

  pub fn save_device(&mut self, peer_id: PeerId, device: Device, paired: Option<bool>) {
    self.devices.insert(peer_id, (device, paired));
  }

  pub fn get_saved_devices(&self) -> &HashMap<PeerId, (Device, Option<bool>)> {
    &self.devices
  }

  pub fn check_paired(&self, peer_id: PeerId) -> Option<&(Device, Option<bool>)> {
    self.devices.get(&peer_id)
  }

  pub fn get_device(&self, peer_id: &PeerId) -> Option<&(Device, Option<bool>)> {
    self.devices.get(peer_id)
  }

  pub fn get_device_mut(&mut self, peer_id: &PeerId) -> Option<&mut (Device, Option<bool>)> {
    self.devices.get_mut(peer_id)
  }

  pub fn remove_device(&mut self, peer_id: &PeerId) -> Option<(Device, Option<bool>)> {
    self.devices.remove(peer_id)
  }

  pub fn contains_device(&self, peer_id: &PeerId) -> bool {
    self.devices.contains_key(peer_id)
  }

  pub fn set_paired(&mut self, peer_id: PeerId, paired: bool) {
    if let Some((_, p)) = self.devices.get_mut(&peer_id) {
      *p = Some(paired);
    }
  }
}
