use std::{
  fmt::Debug,
  process::exit,
  sync::{atomic::AtomicBool, Arc},
  time::Duration,
};

use log::{debug, error, info, trace, warn};
use multiconnect_core::Packet;
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpSocket,
  sync::{
    broadcast::{self},
    mpsc, Notify,
  },
  time::sleep,
};

pub type SharedDaemon = Arc<Daemon>;

#[derive(Debug, Clone)]
pub enum DaemonEvent {
  PacketReceived { packet: Packet },
  DaemonConnected,
  DaemonDisconected,
}

#[derive(Debug, Clone)]
pub enum DaemonCommand {
  SendPacket { packet: Packet },
}

#[derive(Debug)]
pub struct Daemon {
  /// Incoming packet receiver from daemon
  event_rx: broadcast::Receiver<DaemonEvent>,

  /// Outgoing packet sender to daemon
  command_tx: mpsc::Sender<DaemonCommand>,

  /// If we are connected to the daemon
  is_connected: Arc<AtomicBool>,

  retry_notify: Arc<Notify>,
}

impl Daemon {
  /// Connect to the daemon and establish a [`TcpStream`]
  pub async fn connect(port: &u16) -> Result<SharedDaemon, Box<dyn std::error::Error>> {
    let (event_tx, event_rx) = broadcast::channel::<DaemonEvent>(100);
    let (command_tx, mut command_rx) = mpsc::channel::<DaemonCommand>(100);

    let port = *port;
    let is_connected = Arc::new(AtomicBool::new(false));
    let retry_notify = Arc::new(Notify::new());
    let retry_notify_clone = retry_notify.clone();

    let is_connected_clone = is_connected.clone();
    tokio::spawn(async move {
      let mut backoff_secs = 2;

      loop {
        if is_connected.load(std::sync::atomic::Ordering::SeqCst) {
          event_tx.send(DaemonEvent::DaemonDisconected);
        }
        is_connected.store(false, std::sync::atomic::Ordering::SeqCst);

        // try connect
        let socket = match TcpSocket::new_v4() {
          Ok(s) => s,
          Err(e) => {
            error!("Failed to create socket: {}", e);
            exit(-1)
          }
        };

        match socket.connect(format!("127.0.0.1:{}", port).parse().unwrap()).await {
          Ok(stream) => {
            info!("Connected to daemon");
            is_connected.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = event_tx.send(DaemonEvent::DaemonConnected); // TODO
            backoff_secs = 2; // reset backoff on success

            let (mut read_half, mut write_half) = stream.into_split();

            loop {
              tokio::select! {
                res = read_half.read_u16() => {
                  match res {
                    Ok(len) => {
                      let mut raw: Vec<u8> = vec![0u8; len.into()];
                      match read_half.read_exact(&mut raw).await {
                        Ok(_) => {
                          match Packet::from_bytes(&raw) {
                            Ok(packet) => {
                              trace!("Received packet: {:?}", packet);
                              if let Err(e) = event_tx.send(DaemonEvent::PacketReceived { packet }) {
                                error!("Local broadcast error: {}", e);
                              }
                            }
                            Err(e) => error!("Packet decode error: {}", e),
                          }
                        }
                        Err(e) => {
                          warn!("Read error, will reconnect: {}", e);
                          break; // break inner loop → reconnect
                        }
                      };
                    }
                    Err(_) => {
                      warn!("Connection closed by peer");
                      break; // reconnect
                    }
                  }
                }
                res = command_rx.recv() => if let Some(res) = res {
                  match res {
                    DaemonCommand::SendPacket { packet } => {
                      let bytes = match Packet::to_bytes(&packet) {
                        Ok(b) => b,
                        Err(e) => {
                          error!("Serialization error: {}", e);
                          continue;
                        }
                      };
                      if let Err(e) = write_half.write_all(&bytes).await {
                        warn!("Write error: {}", e);
                        break; // reconnect
                      }
                      let _ = write_half.flush().await;
                    }
                  }
                }
              }
            }
          }
          Err(e) => {
            warn!("Failed to connect to daemon: {}, retrying in {}s", e, backoff_secs);
            // sleep(Duration::from_secs(backoff_secs)).await;
            tokio::select! {
              _ = sleep(Duration::from_secs(backoff_secs)) => {
                backoff_secs = (backoff_secs * 2).min(60);
              }
              _ = retry_notify.notified() => {
                warn!("Retry-now signal received, reconnecting immediately");
                backoff_secs = 2; // reset backoff after manual retry
              }
            }
          }
        }
      }
    });

    Ok(Arc::new(Self { event_rx, command_tx, is_connected: is_connected_clone, retry_notify: retry_notify_clone }))
  }

  pub fn reconnect(&self) {
    self.retry_notify.notify_one();
  }

  /// Send a command
  pub fn command_channel(&self) -> mpsc::Sender<DaemonCommand> {
    self.command_tx.clone()
  }

  /// Get a stream of incoming packets
  pub fn event_channel(&self) -> broadcast::Receiver<DaemonEvent> {
    self.event_rx.resubscribe()
  }

  /// Check if the frontend is connected
  pub fn is_connected(&self) -> bool {
    self.is_connected.load(std::sync::atomic::Ordering::SeqCst)
  }
}
