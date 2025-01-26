pub mod networking;

use std::{collections::VecDeque, error::Error, pin::Pin, sync::Arc};

use log::{debug, error, info, trace};
use multiconnect_protocol::daemon::packets::{Acknowledge, Packet, Ping};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::{watch, Mutex, Notify},
};

pub type SharedDaemon = Arc<Mutex<Daemon>>;

const PORT: u16 = 10999;

pub struct Daemon {
  listener: TcpListener,
  queue: Arc<Mutex<VecDeque<Packet>>>,
  notify: Arc<Notify>,
}

// TODO: Clean all this up
impl Daemon {
  pub async fn new() -> Result<SharedDaemon, std::io::Error> {
    let port = std::env::var("MC_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(PORT);

    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
      Ok(l) => l,
      Err(e) => {
        error!("Failed to start daemon (is it already running?)");
        return Err(e);
      }
    };

    info!("Daemon listening on 127.0.0.1:{}", port);

    Ok(Arc::new(Mutex::new(Self {
      listener,
      queue: Arc::new(Mutex::new(VecDeque::new())),
      notify: Arc::new(Notify::new()),
    })))
  }

  pub async fn start(&self) -> Result<(), Box<dyn Error>> {
    while let Ok((stream, addr)) = self.listener.accept().await {
      info!("New connection from {}", addr);
      let queue: Arc<Mutex<VecDeque<Packet>>> = Arc::clone(&self.queue);
      let notify = Arc::clone(&self.notify);

      tokio::spawn(async move { Self::handle(stream, queue, notify).await });
    }

    Ok(())
  }

  async fn handle(stream: TcpStream, queue: Arc<Mutex<VecDeque<Packet>>>, notify: Arc<Notify>) {
    let (mut read_half, mut write_half) = stream.into_split();

    let queue_clone: Arc<Mutex<VecDeque<Packet>>> = Arc::clone(&queue);
    let notify_clone = Arc::clone(&notify);

    let (shutdown_tx, mut shutdown_rx) = watch::channel(());
    let mut shutdown_rx_clone = watch::Receiver::clone(&shutdown_rx);

    let read_task = tokio::spawn({
      let shutdown_tx = shutdown_tx.clone();
      async move {
        loop {
          tokio::select! {
            result = read_half.read_u16() => {
              match result {
                Ok(len) => {
                  trace!("Len: {}", len);
                  if len > u16::MAX {
                    error!("Packet is to big: {}", len);
                    continue;
                  }

                  let mut raw: Vec<u8> = vec![0u8; len.into()];
                  match read_half.read_exact(&mut raw).await {
                  Ok(_) => {
                    trace!("Bytes: {:?}", raw);
                    let packet = match Packet::from_bytes(&raw) {
                    Ok(p) => p,
                    Err(e) => {
                      error!("Error decoding packet {}", e);
                      continue;
                    }
                  };

                  debug!("Received {:?} packet", packet);

                  let queue = Arc::clone(&queue_clone);
                  match packet {
                      Packet::Ping(ping) => {
                        queue.lock().await.push_front(Packet::Acknowledge(Acknowledge::new(ping.id)));
                        notify_clone.notify_one();
                      }
                      _ => {
                        error!("Received unexpected packet")
                      }
                    }
                  }
                  Err(e) => {
                    error!("Read error: {}", e);
                    break;
                  }
                }
              }
              Err(_) => {
                  info!("Connection closed by peer");
                  break;
                }
              }
            }
            _ = shutdown_rx_clone.changed() => {
              break;
            }
          }
        }
        let _ = shutdown_tx.send(());
      }
    });

    let write_task = tokio::spawn(async move {
      loop {
        tokio::select! {
          _ = notify.notified() => {
            info!("NOTI");
            let mut locked = queue.lock().await;
            while let Some(packet) = locked.pop_back() {
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
            }
          }
          _ = shutdown_rx.changed() => {
            info!("Shutting down stream for");
            break;
          }
        }
      }
    });

    let _ = tokio::try_join!(read_task, write_task);
  }

  pub async fn add_to_queue(&mut self, packet: Packet) {
    info!("Adding to queue");
    self.queue.lock().await.push_front(packet);
    self.notify.notify_one();
    info!("Added to queue");
  }
}
