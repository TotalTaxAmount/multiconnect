use std::{collections::VecDeque, error::Error, pin::Pin, sync::Arc};

use log::{debug, error, info};
use multiconnect_protocol::daemon::packets::{Acknowledge, Packet, Ping};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::Mutex,
};

pub mod networking;

const PORT: u16 = 10999;

pub struct Daemon {
  listener: TcpListener,
  queue: Arc<Mutex<VecDeque<Packet>>>,
}

impl Daemon {
  pub async fn new() -> Result<Self, std::io::Error> {
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", PORT)).await {
      Ok(l) => l,
      Err(e) => {
        error!("Failed to start daemon (is it already running?)");
        return Err(e);
      }
    };

    info!("Daemon listening on 127.0.0.1:{}", PORT);

    Ok(Self { listener, queue: Arc::new(Mutex::new(VecDeque::new())) })
  }

  pub async fn start(&self) -> Result<(), Box<dyn Error>> {
    while let Ok((stream, addr)) = self.listener.accept().await {
      info!("New connection from {}", addr);
      let queue_clone = self.queue.clone();

      tokio::spawn(async move { Self::handle(stream, queue_clone).await });
    }

    Ok(())
  }

  async fn handle(stream: TcpStream, queue: Arc<Mutex<VecDeque<Packet>>>) {
    let (mut read_half, mut write_half) = stream.into_split();

    let queue_clone = queue.clone();
    let read_task = tokio::spawn(async move {
      'main: loop {
        let mut raw: Vec<u8> = Vec::new();
        let mut buf = [0; 4096];
        while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
          let len = match read_half.read(&mut buf).await {
            Ok(0) => {
              info!("Connection closed by peer");
              break 'main;
            },
            Ok(len) => len,
            Err(e) => {
              error!("Read error: {}", e);
              break;
            }
          };

          raw.extend_from_slice(&buf[..len]);
        }

        if raw.is_empty() { continue; } 
        let packet = match Packet::from_bytes(&raw[..(&raw.len() - 4)]) {
          Ok(p) => p,
          Err(e) => {
            error!("Error decoding packet {}", e);
            continue;
          }
        };

        debug!("Received {:?} packet", packet);

        let mut locked = queue_clone.lock().await;
        match packet {
          Packet::Ping(ping) => {
            locked.push_front(Packet::Acknowledge(Acknowledge::new(ping.id)));
          }
          Packet::PeerFound(peer_found) => todo!(),
          Packet::PeerPairRequest(peer_pair_request) => todo!(),
          Packet::PeerConnect(peer_connect) => todo!(),
          Packet::TransferStart(transfer_start) => todo!(),
          Packet::TransferChunk(transfer_chunk) => todo!(),
          Packet::TransferEnd(transfer_end) => todo!(),
          Packet::TransferStatus(transfer_status) => todo!(),
          Packet::SmsMessage(sms_message) => todo!(),
          Packet::Notify(notify) => todo!(),
          _ => {
            error!("Received unexpected packet")
          }
        }
      }
    });

    let write_task = tokio::spawn(async move {
      loop {
        let mut locked = queue.lock().await;
        if let Some(packet) = locked.pop_back() {
          debug!("Sending {:?} packet", packet);
          let bytes = Packet::to_bytes(packet);
          match bytes {
            Ok(b) => {
              if let Err(e) = write_half.write_all(&b).await {
                error!("Write error: {}", e);
                break;
              };
              let _ = write_half.flush().await;
            }
            Err(_) => todo!(),
          }
        } else {
          tokio::task::yield_now().await;
        }
      }
    });

    let _ = tokio::try_join!(read_task, write_task);
  }
}
