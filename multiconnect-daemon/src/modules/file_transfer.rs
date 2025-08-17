use async_trait::async_trait;
use dashmap::DashMap;
use libp2p::{
  futures::{channel::mpsc, SinkExt, StreamExt},
  PeerId,
};
use log::{debug, info, warn};
use multiconnect_config::CONFIG;
use sha2::{Digest, Sha256};
use std::{
  collections::HashMap,
  error::Error,
  io::{self, BufReader, Read},
  path::{Path, PathBuf},
  str::FromStr,
  sync::Arc,
  u16,
};
use tokio::{
  fs::{self, File, OpenOptions},
  io::{AsyncReadExt, AsyncWriteExt},
  sync::Mutex,
};
use uuid::Uuid;

use multiconnect_core::{
  generated::{P4TransferStart, P5TransferChunk, P6TransferStatus},
  local::transfer::{l10_transfer_progress, l11_transfer_status, L10TransferProgress, L11TransferStatus},
  Packet,
};

use crate::{networking::NetworkEvent, FrontendEvent};

use super::{MulticonnectCtx, MulticonnectModule};

pub struct FileTransferModule {
  transfers: Arc<DashMap<Uuid, FileTransfer>>,
  transfer_tx: mpsc::Sender<FileTransfer>,
  transfer_rx: Option<mpsc::Receiver<FileTransfer>>,
  chunk_tx: mpsc::Sender<P5TransferChunk>,
  chunk_rx: Option<mpsc::Receiver<P5TransferChunk>>,
}

const METADATA_SIZE: usize = 257;

#[derive(Debug, Clone)]
enum TransferDirection {
  Inbound,
  Outbound,
}

#[derive(Debug, Clone)]
struct FileTransfer {
  /// The uuid of the transfer
  uuid: Uuid,
  /// Total size of the file
  total_len: usize,
  /// The amount of the file that has been sent or recived
  processed_len: usize,
  /// The peer id of the sender or the reciver depending on the direction
  peer: PeerId,
  /// The file name
  file: String,
  /// The hash of the file
  hash: Option<String>,
  /// The direction of the transfer
  direction: TransferDirection,
}

impl FileTransfer {
  pub fn new(
    uuid: Uuid,
    len: usize,
    peer: PeerId,
    file: impl Into<String>,
    hash: Option<String>,
    direction: TransferDirection,
  ) -> Self {
    Self { uuid, total_len: len, processed_len: 0, peer, file: file.into(), hash, direction }
  }
}

impl FileTransferModule {
  pub async fn new() -> Self {
    let (transfer_tx, transfer_rx) = mpsc::channel::<FileTransfer>(10);
    let (chunk_tx, chunk_rx) = mpsc::channel::<P5TransferChunk>(100);

    Self {
      transfers: Arc::new(DashMap::new()),
      transfer_rx: Some(transfer_rx),
      transfer_tx,
      chunk_tx,
      chunk_rx: Some(chunk_rx),
    }
  }

  /// Calculate the sha256 checksum of a file
  async fn hash_sha256<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
    debug!("Hashing file: {}", path.as_ref().display());
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1024 * 1024];

    loop {
      let read = reader.read(&mut buf)?;
      if read == 0 {
        break;
      }
      hasher.update(&buf[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
  }

  /// Generate a unique file path from a file path
  /// Will check if file exists, if it does it will chage the name until the file doesnt exist
  /// For example, say the path is `/foo/bar.png` and it exists. The target path will become
  /// `/foo/bar (1).png` if that exists then `/foo/bar (2).png` and so on
  async fn generate_unique_file<P: AsRef<Path>>(base: P) -> io::Result<PathBuf> {
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

    Ok(path)
  }
}

#[async_trait]
impl MulticonnectModule for FileTransferModule {
  async fn on_network_event(&mut self, event: NetworkEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>> {
    // Only thing we care about is recivied packets from peers
    if let NetworkEvent::PacketReceived(source, packet) = event {
      match packet {
        // A peer has started a transfer to us
        Packet::P4TransferStart(packet) => {
          // Get the uuid for the transfer provided by the peer
          let uuid = Uuid::from_str(&packet.uuid)?;
          let cfg = &CONFIG.get().ok_or("Failed to get config")?.read().await;

          // Figure out where to save the file
          let base = Path::new(&cfg.get_config().modules.transfer.save_path); // Base path from config

          let file_name =
            Path::new(&packet.file_name).file_name().ok_or("Failed to get filename from path")?.to_os_string(); // File name from the peer

          let full_path = base.join(file_name); // The full save path

          let mut path = Self::generate_unique_file(&full_path).await?.to_string_lossy().to_string(); // Make sure we are not going to overwrite something
          path.push_str(".tmp");

          debug!("Received transfer start from {} for {:?}: uuid = {}", source, path, uuid);

          // Add transfer to know transfers
          self.transfers.insert(
            uuid,
            FileTransfer::new(
              uuid,
              packet.file_size as usize,
              source,
              path,
              Some(packet.signature),
              TransferDirection::Inbound,
            ),
          );
        }
        Packet::P5TransferChunk(packet) => {
          // Send transfer packet to other thread so we don't block other modules
          self.chunk_tx.send(packet).await?;
        }
        Packet::P6TransferStatus(packet) => match packet.status() {
          // A peer recivied a file successfully
          multiconnect_core::generated::p6_transfer_status::PtStatus::Ok => {
            // Parse the uuid
            let uuid = Uuid::from_str(&packet.uuid)?;
            let file_name =
              Path::new(&self.transfers.remove(&uuid).ok_or(format!("No active transfer for uuid = {}", uuid))?.1.file)
                .file_name()
                .ok_or("Failed to get filename")?
                .to_string_lossy()
                .to_string();

            debug!("Recivied sucesssful status for transfer: {}", file_name);

            // Notify that frontend
            ctx
              .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                file_name,
                l11_transfer_status::LtStatus::Ok,
              )))
              .await;
          }
          multiconnect_core::generated::p6_transfer_status::PtStatus::MalformedPacket => todo!(),
          // Signatures did not match and the peer did not save the file
          multiconnect_core::generated::p6_transfer_status::PtStatus::WrongSig => {
            // Parse the uuid
            let uuid = Uuid::from_str(&packet.uuid)?;

            // Remove the transfer from active transfers
            let file_name =
              Path::new(&self.transfers.remove(&uuid).ok_or(format!("No active transfer for uuid = {}", uuid))?.1.file)
                .file_name()
                .ok_or("Failed to get filename")?
                .to_string_lossy()
                .to_string();

            // Notify the frontend
            ctx
              .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                file_name,
                l11_transfer_status::LtStatus::InvalidSig,
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
          // Get the size of the file we want to send
          let len = fs::metadata(&packet.file_path).await?.len() as usize;
          // Target peer address
          let peer_id = PeerId::from_str(&packet.target)?;

          let uuid = Uuid::new_v4();

          debug!("Sending on channel");
          let transfer = FileTransfer::new(
            uuid,
            len,
            peer_id,
            packet.file_path,
            None, /* Hash is none for now */
            TransferDirection::Outbound,
          );

          self.transfers.insert(uuid, transfer.clone());
          self.transfer_tx.send(transfer).await?;
        }
        _ => {}
      }
    }
    Ok(())
  }

  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>) -> Result<(), Box<dyn Error>> {
    let mut transfer_rx = self.transfer_rx.take().unwrap();
    let mut chunk_rx = self.chunk_rx.take().unwrap();

    let transfers = self.transfers.clone();

    // Thread to handle reading and writing packets
    tokio::spawn(async move {
      loop {
        tokio::select! {
          // We have a transfer to send
          transfer = transfer_rx.next() => if let Some(transfer) = transfer {
            // Now we get the hash when we wont block other things
            let hash = match Self::hash_sha256(&transfer.file).await {
              Ok(hash) => hash,
              Err(e) => {
                warn!("Failed to hash file {}: {}", transfer.file, e);
                // Notify frontend
                let guard = ctx.lock().await;
                guard
                  .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                    transfer.file.clone(),
                    l11_transfer_status::LtStatus::InvalidFile,
                  )))
                  .await;
                continue;
              }
            };

            debug!("Starting transfer: {}, sig = {}", transfer.file, hash);

            // Create a uuid for this transfer

            // Get this size for a chunk, make sure to leave room for the reset of the transfer packet
            let chunk_size = u16::MAX as usize - METADATA_SIZE;
            let ctx = ctx.clone();
            let transfer_clone = transfer.clone();

            // Do the acuall transfer
            let result: Result<(), String> = async move {
              let path = Path::new(&transfer.file);

              debug!("Sending start packet");

              // Send transfer start packet
              ctx
                .lock()
                .await
                .send_to_peer(
                  transfer.peer,
                  Packet::P4TransferStart(P4TransferStart::new(
                    transfer.total_len as u64,
                    transfer.file.clone(),
                    transfer.uuid.to_string(),
                    hash /* Now we have the hash */,
                  )),
                )
                .await;


              // Open the file we want to send
              let mut file = File::open(path).await.map_err(|e| e.to_string())?;

              // Create a buffer to write into
              let mut buf = vec![0u8; chunk_size];

              // Current amout processed
              let mut processed = 0;
              loop {
                // Read into the buffer
                let read = file.read(&mut buf).await.map_err(|e| e.to_string())?;
                if read == 0 {
                  debug!("Done reading file");
                  // We have read the whole file
                  break;
                }
                // Add the amout read to the total amount processed
                processed += read as u64;

                // Data to send
                let data = buf[..read].to_vec();
                let guard = ctx.lock().await;

                // Send chunk to targret peer
                debug!("Sending chunk with {} bytes", read);
                guard
                  .send_to_peer(transfer.peer.clone(), Packet::P5TransferChunk(P5TransferChunk::new(transfer.uuid, data)))
                  .await;

                // Update the frontend progress
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
            }.await;

            if let Err(e) = result {
              warn!("Failed to send file {} to peer {}: {}", transfer_clone.file, transfer_clone.peer, e);
            }
          },
          // Recivied a chunk for an incoming transfer
          chunk_packet = chunk_rx.next() => if let Some(chunk_packet) = chunk_packet {
            debug!("Recivied chunk");
            // Get the uuid of the transfer from the packet
            let uuid = Uuid::from_str(&chunk_packet.uuid).unwrap();

            // Get the transfer (added from P4TransferStart)
            if let Some(mut transfer) = transfers.get_mut(&uuid) {
              // Get the path to save the file (currenttly it is a temp file) and open it
              let file_clone = transfer.file.clone();
              let path = Path::new(&file_clone);
              let path_string = path.to_string_lossy().to_string();
              let mut file = OpenOptions::new().create(true).append(true).open(&path).await.unwrap();

              // Write the data from the chunk packet to the file
              if let Ok(len) = file.write(&chunk_packet.data).await {
                // Update transfer progress
                transfer.processed_len += len;

                // Check if all data has been transfered
                if transfer.processed_len == transfer.total_len {
                  // Get the real signature
                  let sig = Self::hash_sha256(path).await.unwrap();
                  // Get the expected signature
                  let hash = transfer.hash.clone().ok_or("Failed to get hash").unwrap();

                  // Confirm signatures match
                  if sig == hash {
                    // Figure out the final save path
                    let file_name = path.file_name().ok_or("Failed to get filename").unwrap().to_string_lossy();
                    let file_name = file_name.strip_suffix(".tmp").ok_or("Expected .tmp suffix").unwrap();
                    let parent = path.parent().ok_or("Failed to get parent directory").unwrap();
                    let final_path = parent.join(file_name);

                    // Move temp download file to the final path
                    tokio::fs::rename(path, &final_path).await.unwrap();
                    debug!("Successfully saved file ({} bytes)", transfer.total_len);

                    // Notify frontend
                    let guard = ctx.lock().await;
                    guard
                      .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                        path_string,
                        l11_transfer_status::LtStatus::Ok,
                      )))
                      .await;

                    guard.send_to_peer(transfer.peer, Packet::P6TransferStatus(P6TransferStatus::new(uuid, multiconnect_core::generated::p6_transfer_status::PtStatus::Ok))).await;
                  } else {
                    warn!("File signature doesnt match: {} != {}", sig, hash);
                    // Delete whatever was downloaded
                    let _ = fs::remove_file(path).await;

                    // Notify frontend
                    let guard = ctx.lock().await;
                    guard
                      .send_to_frontend(Packet::L11TransferStatus(L11TransferStatus::new(
                        path_string,
                        l11_transfer_status::LtStatus::InvalidSig,
                      )))
                      .await;

                    guard.send_to_peer(transfer.peer, Packet::P6TransferStatus(P6TransferStatus::new(uuid, multiconnect_core::generated::p6_transfer_status::PtStatus::WrongSig))).await;
                  }
                  drop(transfer);
                  transfers.remove(&uuid);
                } else {
                  // Update frontend progress
                  let guard = ctx.lock().await;
                    guard
                    .send_to_frontend(Packet::L10TransferProgress(L10TransferProgress::new(
                      path_string,
                      transfer.total_len as u64,
                      transfer.processed_len as u64,
                      l10_transfer_progress::Direction::Inbound,
                    )))
                    .await;
                }
              } else {
                warn!("Failed to write file");
              }
            } else {
              warn!("No transfer with uuid: {}", uuid);
            }
          }
        }
      }
    });
    Ok(())
  }
}
