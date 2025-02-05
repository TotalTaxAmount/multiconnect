use std::{
  collections::HashMap,
  ffi::OsStr,
  fs::{File, OpenOptions},
  io::{Read, Write},
  path::PathBuf,
  str::FromStr,
};

use libp2p::{identity::Keypair, PeerId};

use crate::config::CONFIG;

pub struct KeyStore {
  keys: HashMap<PeerId, Keypair>,
}

impl KeyStore {
  pub fn new() -> Self {
    Self { keys: Self::load_keys() }
  }

  fn load_keys() -> HashMap<PeerId, Keypair> {
    let keyfile = CONFIG.get_config_dir().join("saved");
    if !keyfile.exists() {
      return HashMap::new();
    }

    let mut file = File::open(keyfile).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    let keys: HashMap<String, Vec<u8>> = bincode::deserialize(&buf).unwrap();
    let mut key_map = HashMap::new();

    for (peer_id_string, key_bytes) in keys {
      let peer_id = PeerId::from_str(&peer_id_string).unwrap();
      let keypair = Keypair::from_protobuf_encoding(&key_bytes).unwrap();
      key_map.insert(peer_id, keypair);
    }

    key_map
  }

  fn save_keys(&self) {
    let keyfile = CONFIG.get_config_dir().join("saved");
    let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(keyfile).unwrap();

    let mut keys_to_save = HashMap::new();

    for (peer_id, keypair) in &self.keys {
      let peer_id_string = peer_id.to_string();
      let key_bytes = keypair.to_protobuf_encoding().unwrap();
      keys_to_save.insert(peer_id_string, key_bytes);
    }

    let serialized = bincode::serialize(&keys_to_save).unwrap();
    file.write_all(&serialized).unwrap();
  }

  pub fn store_key(&mut self, peer_id: PeerId, keypair: Keypair) {
    self.keys.insert(peer_id, keypair);
    self.save_keys();
  }

  pub fn retrieve_key(&self, peer_id: PeerId) -> Option<&Keypair> {
    self.keys.get(&peer_id)
  }

  pub fn remove_key(&mut self, peer_id: PeerId) -> Option<Keypair> {
    let key = self.keys.remove(&peer_id);
    self.save_keys();
    key
  }
}
