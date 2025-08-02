use async_trait::async_trait;
use libp2p::{
  futures::{channel::mpsc, SinkExt, StreamExt},
  PeerId,
};
use log::{debug, warn};
use multiconnect_config::CONFIG;
use sha2::{Digest, Sha256};
use std::{
  collections::HashMap,
  error::Error,
  io,
  path::{Path, PathBuf},
  str::FromStr,
  sync::Arc,
  u16,
};
use tokio::{
  fs::{self, File, OpenOptions},
  io::{AsyncReadExt, AsyncWriteExt, BufReader},
  sync::Mutex,
};
use uuid::Uuid;

use multiconnect_protocol::{
  generated::{P4TransferStart, P5TransferChunk},
  local::transfer::{l10_transfer_progress, l11_transfer_status, L10TransferProgress, L11TransferStatus},
  Packet,
};

use crate::{networking::NetworkEvent, FrontendEvent};

use super::{MulticonnectCtx, MulticonnectModule};

pub struct FileTransferModule {
  transfers: Arc<Mutex<HashMap<Uuid, FileTransfer>>>,
  transfer_tx: mpsc::Sender<FileTransfer>,
  transfer_rx: Option<mpsc::Receiver<FileTransfer>>,
  chunk_tx: mpsc::Sender<P5TransferChunk>,
  chunk_rx: Option<mpsc::Receiver<P5TransferChunk>>,
}

#[derive(Debug, Clone)]
struct FileTransfer {
  total_len: usize,
  processed_len: usize,
  peer: PeerId, // Sender or recivers
  file: String,
  hash: Option<String>,
}

impl FileTransfer {
  pub fn new(len: usize, peer: PeerId, file: impl Into<String>, hash: Option<String>) -> Self {
    Self { total_len: len, processed_len: 0, peer, file: file.into(), hash }
  }
}

impl FileTransferModule {
  pub async fn new() -> Self {
    let (transfer_tx, transfer_rx) = mpsc::channel::<FileTransfer>(10);
    let (chunk_tx, chunk_rx) = mpsc::channel::<P5TransferChunk>(100);

    Self {
      transfers: Arc::new(Mutex::new(HashMap::new())),
      transfer_rx: Some(transfer_rx),
      transfer_tx,
      chunk_tx,
      chunk_rx: Some(chunk_rx),
    }
  }

  async fn hash_sha256<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
    let file = File::open(path).await?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 4096];
    debug!("Hashing file");

    loop {
      let read = reader.read(&mut buf).await?;
      if read == 0 {
        break;
      }
      hasher.update(&buf[..read]);
    }
    debug!("Done");

    Ok(hex::encode(hasher.finalize()))
  }

  async fn create_unique_file<P: AsRef<Path>>(base: P) -> io::Result<PathBuf> {
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

    File::create(&path).await?;
    Ok(path)
  }
}

#[async_trait]
impl MulticonnectModule for FileTransferModule {
  async fn on_network_event(&mut self, event: NetworkEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>> {
    if let NetworkEvent::PacketReceived(source, packet) = event {
      match packet {
        Packet::P4TransferStart(packet) => {
          let uuid = Uuid::from_str(&packet.uuid)?;
          let cfg = &CONFIG.get().ok_or("Failed to get config")?.read().await;
          let base = Path::new(&cfg.get_config().modules.transfer.save_path);

          let mut file_name =
            Path::new(&packet.file_name).file_name().ok_or("Failed to get filename from path")?.to_os_string();
          file_name.push(".tmp");

          let full_path = base.join(file_name);

          let path = Self::create_unique_file(&full_path).await?;

          debug!("Received transfer start from {} for {:?}: uuid = {}", source, path, uuid);

          self.transfers.lock().await.insert(
            uuid,
            FileTransfer::new(packet.file_size as usize, source, path.to_string_lossy(), Some(packet.signature)),
          );
        }
        Packet::P5TransferChunk(packet) => {
          self.chunk_tx.send(packet).await?;
        }
        Packet::P6TransferStatus(packet) => match packet.status() {
          multiconnect_protocol::generated::p6_transfer_status::Status::Ok => {
            let uuid = Uuid::from_str(&packet.uuid)?;
            let file_name = Path::new(
              &self.transfers.lock().await.remove(&uuid).ok_or(format!("No active transfer for uuid = {}", uuid))?.file,
            )
            .file_name()
            .ok_or("Failed to get filename")?
            .to_string_lossy()
            .to_string();
            ctx
              .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                file_name,
                l11_transfer_status::Status::Ok,
              )))
              .await;
          }
          multiconnect_protocol::generated::p6_transfer_status::Status::MalformedPacket => todo!(),
          multiconnect_protocol::generated::p6_transfer_status::Status::WrongSig => {
            let uuid = Uuid::from_str(&packet.uuid)?;
            let file_name = Path::new(
              &self.transfers.lock().await.remove(&uuid).ok_or(format!("No active transfer for uuid = {}", uuid))?.file,
            )
            .file_name()
            .ok_or("Failed to get filename")?
            .to_string_lossy()
            .to_string();
            ctx
              .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                file_name,
                l11_transfer_status::Status::InvalidSig,
              )))
              .await;
          }
        },

        _ => {}
      }
    }

    Ok(())
  }

  async fn on_frontend_event(
    &mut self,
    event: FrontendEvent,
    _ctx: &mut MulticonnectCtx,
  ) -> Result<(), Box<dyn Error>> {
    if let FrontendEvent::RecvPacket(packet) = event {
      match packet {
        Packet::L9TransferFile(packet) => {
          debug!("Received command to send file from fronted");
          let len = fs::metadata(&packet.file_path).await?.len() as usize;
          let peer_id = PeerId::from_str(&packet.target)?;
          // let hash = Self::hash_sha256(Path::new(&packet.file_path)).await?;

          debug!("Sending on channel");
          self.transfer_tx.send(FileTransfer::new(len, peer_id, packet.file_path, None)).await?;
        }
        _ => {}
      }
    }
    Ok(())
  }

  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>) -> Result<(), Box<dyn Error>> {
    let mut transfer_rx = self.transfer_rx.take().ok_or("Could not get transfer rx alone")?;
    let mut chunk_rx = self.chunk_rx.take().ok_or("Could not get transfer rx alone")?;

    let transfers = self.transfers.clone();

    tokio::spawn(async move {
      loop {
        tokio::select! {
          transfer = transfer_rx.next() => if let Some(transfer) = transfer {
            let hash = Self::hash_sha256(&transfer.file).await.unwrap();
            debug!("Starting transfer: {}, sig = {}", transfer.file, hash);
            let transfer_uuid = Uuid::new_v4();
            let path = Path::new(&transfer.file);
            let chunk_size = u16::MAX as usize - 257;

            let result: Result<(), Box<dyn Error>> = async {
              debug!("Sending start packet");
              ctx
                .lock()
                .await
                .send_to_peer(
                  transfer.peer,
                  Packet::P4TransferStart(P4TransferStart::new(
                    transfer.total_len as u64,
                    transfer.file.clone(),
                    transfer_uuid.to_string(),
                    hash,
                  )),
                )
                .await;

              let mut file = File::open(path).await?;
              let mut buf = vec![0u8; chunk_size];

              let mut processed = 0;
              loop {
                let read = file.read(&mut buf).await?;
                if read == 0 {
                  break;
                }
                processed += read as u64;

                let data = buf[..read].to_vec();
                let guard = ctx.lock().await;

                guard
                  .send_to_peer(transfer.peer.clone(), Packet::P5TransferChunk(P5TransferChunk::new(transfer_uuid, data)))
                  .await;

                guard
                  .send_to_frontend(Packet::L10TransferProgress(L10TransferProgress::new(
                    transfer.file.clone(),
                    transfer.total_len as u64,
                    processed,
                    l10_transfer_progress::Direction::Outbound,
                  )))
                  .await;
              }

              Ok(())
            }
            .await;

            if let Err(e) = result {
              warn!("Failed to send file {} to peer {}: {}", transfer.file, transfer.peer, e);
            }
          },
          chunk = chunk_rx.next() => if let Some(chunk) = chunk {
            debug!("Recivied chunk");
            let uuid = Uuid::from_str(&chunk.uuid).unwrap();
            if let Some(status) = transfers.lock().await.get_mut(&uuid) {
              let path = Path::new(&status.file);
              let mut file = OpenOptions::new().append(true).open(&path).await.unwrap();
              if let Ok(len) = file.write(&chunk.data).await {
                status.processed_len += len;
                let hash = status.hash.clone().ok_or("Failed to get hash").unwrap();

                if status.processed_len == status.total_len {
                  let sig = Self::hash_sha256(path).await.unwrap();
                  if sig == hash{
                    let parent = path.parent().ok_or("Failed to get parent directory").unwrap();
                    let file_name = path.file_name().ok_or("Failed to get filename").unwrap().to_string_lossy();
                    let file_name = file_name.strip_suffix(".tmp").ok_or("Expected .tmp suffix").unwrap();
                    let final_path = parent.join(file_name);

                    tokio::fs::rename(path, &final_path).await.unwrap();
                    debug!("Successfully saved file ({} bytes)", status.total_len);
                    let guard = ctx.lock().await;
                    guard
                      .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                        path.file_name().ok_or("Failed to get filename").unwrap().to_string_lossy().to_string(),
                        l11_transfer_status::Status::Ok,
                      )))
                      .await;

                    // ctx.send_to_peer(, packet)
                  } else {
                    warn!("File signature doesnt match: {} != {}", sig, hash);
                    let _ = fs::remove_file(path).await;
                    let guard = ctx.lock().await;
                    guard
                      .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                        path.file_name().ok_or("Failed to get filename").unwrap().to_string_lossy().to_string(),
                        l11_transfer_status::Status::InvalidSig,
                      )))
                      .await;
                  }

                  transfers.lock().await.remove(&uuid);
                } else {
                  let guard = ctx.lock().await;
                    guard
                    .send_to_frontend(Packet::L10TransferProgress(L10TransferProgress::new(
                      path.file_name().ok_or("Failed to get filename").unwrap().to_string_lossy().to_string(),
                      status.total_len as u64,
                      status.processed_len as u64,
                      l10_transfer_progress::Direction::Inbound,
                    )))
                    .await;
                }
              } else {
                warn!("Failed to write file");
              }
            }
          }
        }
      }
    });
    Ok(())
  }
}
