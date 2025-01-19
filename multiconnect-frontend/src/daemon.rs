use std::{collections::VecDeque, sync::Arc};

use log::{debug, error, info, warn};
use multiconnect_protocol::daemon::packets::{Packet, Ping};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpSocket,
  sync::Mutex,
};

const PORT: u16 = 10999;

pub struct Daemon {
  queue: Arc<Mutex<VecDeque<Packet>>>,
}

impl Daemon {
  pub async fn new() -> Result<Arc<Mutex<Self>>, Box<dyn std::error::Error>> {
    let socket = TcpSocket::new_v4()?;
    let stream = match socket.connect(format!("127.0.0.1:{}", PORT).parse()?).await {
      Ok(s) => s,
      Err(e) => {
        error!("Failed to connect to daemon: {}", e);
        return Err(Box::new(e));
      }
    };
    info!("Connected to daemon");

    let queue = Arc::new(Mutex::new(VecDeque::new()));

    let queue_clone = queue.clone();

    tokio::spawn(async move {
      let (mut read_half, mut write_half) = stream.into_split();

      let read_queue_clone = queue_clone.clone();
      let read_task = tokio::spawn(async move {
        'main: loop {
          let mut raw: Vec<u8> = Vec::new();
          let mut buf = [0; 4096];
          while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
            let len = match read_half.read(&mut buf).await {
              Ok(0) => {
                warn!("Lost connection to daemon");
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

          let packet = match Packet::from_bytes(&raw[..(&raw.len() - 4)]) {
            Ok(p) => p,
            Err(_) => {
              error!("Error decoding packet");
              continue;
            }
          };

          debug!("Received {:?} packet", packet);

          let mut locked = read_queue_clone.lock().await;
          match packet {
            Packet::Acknowledge(acknowledge) => {
              debug!("Received ack for ping req {}", acknowledge.req_id);
            },
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
          };
        }
      });

      let write_task = tokio::spawn(async move {
        loop {
          let mut locked = queue_clone.lock().await;
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
    });


    Ok(Arc::new(Mutex::new(Self { queue })))
  }

  pub async fn ping(&mut self) {
    self.queue.lock().await.push_front(Packet::Ping(Ping::new()));
  }
}
