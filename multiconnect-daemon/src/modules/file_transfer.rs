use async_trait::async_trait;
use libp2p::PeerId;
use log::{debug, info, warn};
use sha2::{Digest, Sha256};
use std::{
  collections::HashMap,
  fs::{self, File},
  io::{self, BufReader, Read, Write},
  path::{Path, PathBuf},
  str::FromStr,
  sync::Arc,
};
use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::Mutex};
use uuid::Uuid;

use multiconnect_protocol::{
  local::transfer::{L10TransferProgress, L11TransferStatus},
  Packet,
};

use crate::{networking::NetworkEvent, FrontendEvent};

use super::{MulticonnectCtx, MulticonnectModule};

pub struct FileTransferModule {
  active_transfers: Arc<Mutex<HashMap<Uuid, FileTransferState>>>,
}

struct FileTransferState {
  total_len: usize,
  processed_len: usize,
  source: PeerId,
  file: String,
  hash: String,
}

impl FileTransferState {
  pub fn new(len: usize, source: PeerId, file: impl Into<String>, hash: impl Into<String>) -> Self {
    Self { total_len: len, processed_len: 0, source, file: file.into(), hash: hash.into() }
  }
}

impl FileTransferModule {
  pub async fn new() -> Self {
    Self { active_transfers: Arc::new(Mutex::new(HashMap::new())) }
  }

  fn hash_sha256(path: &Path) -> std::io::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 4096];

    loop {
      let read = reader.read(&mut buf)?;
      if read == 0 {
        break;
      }
      hasher.update(&buf[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
  }

  fn create_unique_file<P: AsRef<Path>>(base: P) -> io::Result<PathBuf> {
    let base = base.as_ref();
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let stem = base.file_stem().unwrap_or_default().to_string_lossy();
    let ext = base.extension().map(|e| e.to_string_lossy()).unwrap_or(std::borrow::Cow::Borrowed(""));

    let mut path = base.to_path_buf();
    let mut c = 1;

    while path.exists() {
      let filename = format!("{} ({}).{}", stem, c, ext);
      path = parent.join(filename);
      c += 1;
    }

    File::create(&path)?;
    Ok(path)
  }
}

#[async_trait]
impl MulticonnectModule for FileTransferModule {
  async fn on_network_event(&mut self, event: NetworkEvent, ctx: &mut MulticonnectCtx) {
    if let NetworkEvent::PacketRecived(source, packet) = event {
      match packet {
        Packet::P4TransferStart(packet) => {
          let uuid = Uuid::from_str(&packet.uuid).unwrap();
          let path = Self::create_unique_file(&packet.file_name).unwrap();
          debug!("File path: {:?}", path);

          self.active_transfers.lock().await.insert(
            uuid,
            FileTransferState::new(packet.file_size as usize, source, path.to_string_lossy(), packet.signature),
          );
        }
        Packet::P5TransferChunk(packet) => {
          let uuid = Uuid::from_str(&packet.uuid).unwrap();
          if let Some(status) = self.active_transfers.lock().await.get_mut(&uuid) {
            let path = Path::new(&status.file);
            let mut file = OpenOptions::new().append(true).open(&path).await.unwrap();
            if let Ok(len) = file.write(&packet.data).await {
              status.processed_len += len;

              if status.processed_len == status.total_len {
                let sig = Self::hash_sha256(path).unwrap();
                if sig == status.hash {
                  debug!("Sucessfully saved file");
                  ctx
                    .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                      path.file_name().unwrap().to_string_lossy().to_string(),
                      multiconnect_protocol::local::transfer::l11_transfer_status::Status::Ok,
                    )))
                    .await;

                  // ctx.send_to_peer(, packet)
                } else {
                  warn!("File signature doesnt match: {} != {}", sig, status.hash);
                  ctx
                    .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                      path.file_name().unwrap().to_string_lossy().to_string(),
                      multiconnect_protocol::local::transfer::l11_transfer_status::Status::InvalidSig,
                    )))
                    .await;
                }
              } else {
                ctx
                  .send_to_frontend(Packet::L10TransferProgress(L10TransferProgress::new(
                    path.file_name().unwrap().to_string_lossy().to_string(),
                    status.total_len as u64,
                    status.processed_len as u64,
                  )))
                  .await;
              }
            } else {
              warn!("Failed to write file");
            }
          }
        }
        Packet::P6TransferStatus(packet) => match packet.status() {
          multiconnect_protocol::generated::p6_transfer_status::Status::Ok => {
            let uuid = Uuid::from_str(&packet.uuid).unwrap();
            let file_name = Path::new(&self.active_transfers.lock().await.remove(&uuid).unwrap().file)
              .file_name()
              .unwrap()
              .to_string_lossy()
              .to_string();
            ctx
              .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                file_name,
                multiconnect_protocol::local::transfer::l11_transfer_status::Status::Ok,
              )))
              .await;
          }
          multiconnect_protocol::generated::p6_transfer_status::Status::MalformedPacket => todo!(),
          multiconnect_protocol::generated::p6_transfer_status::Status::WrongSig => {
            let uuid = Uuid::from_str(&packet.uuid).unwrap();
            let file_name = Path::new(&self.active_transfers.lock().await.remove(&uuid).unwrap().file)
              .file_name()
              .unwrap()
              .to_string_lossy()
              .to_string();
            ctx
              .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                file_name,
                multiconnect_protocol::local::transfer::l11_transfer_status::Status::InvalidSig,
              )))
              .await;
          }
        },

        _ => {}
      }
    }
  }

  async fn on_frontend_event(&mut self, event: FrontendEvent, ctx: &mut MulticonnectCtx) {
    if let FrontendEvent::RecvPacket(packet) = event {
      match packet {
        Packet::L9TransferFile(packet) => {
          let uuid = Uuid::new_v4();
          let path = Path::new(&packet.file_path);
          let hash = Self::hash_sha256(path).unwrap();
          let size = fs::metadata(path).unwrap().len();

          self.active_transfers.lock().await.insert(
            uuid,
            FileTransferState::new(
              size as usize,
              ctx.this_device.peer,
              path.file_name().unwrap().to_string_lossy(),
              hash,
            ),
          );
        }
        _ => {}
      }
    }
  }

  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>) {}
}
