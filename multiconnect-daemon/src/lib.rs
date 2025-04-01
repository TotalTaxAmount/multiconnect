pub mod modules;
pub mod networking;

use std::{sync::Arc, time::Duration};

use argh::FromArgs;
use log::{debug, error, info, trace};
use multiconnect_protocol::Packet;
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::{
    broadcast::{self, error::RecvError},
    mpsc, watch, Mutex,
  },
  time,
};
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};

pub type SharedDaemon = Arc<Daemon>;

const PORT: u16 = 10999;

#[derive(FromArgs)]
#[argh(help_triggers("-h", "--help"))]
/// Sync devices
pub struct MulticonnectArgs {
  /// specify the port of the daemon to run on (default 10999)
  #[argh(option, default = "PORT", short = 'p')]
  pub port: u16,
  /// specify the log level (default is info) {trace|debug|info|warn|error}
  #[argh(option, default = "String::from(\"info\")")]
  pub log_level: String,
}

#[derive(Debug)]
pub struct Daemon {
  /// The [`TcpListener`] for the daemon
  listener: TcpListener,

  /// Incoming packet sender form clients
  incoming_tx: broadcast::Sender<Packet>,
  /// Incoming packet receiver form clients
  incoming_rx: broadcast::Receiver<Packet>,

  /// Outgoing packet sender to clients
  outgoing_tx: mpsc::Sender<Packet>,
  /// Outgoing packet receiver to clients
  outgoing_rx: Arc<Mutex<mpsc::Receiver<Packet>>>,
}

// TODO: Clean all this up
impl Daemon {
  /// Create a new daemon and bind too a port (`MC_PORT` env var)
  pub async fn new(port: u16) -> Result<SharedDaemon, std::io::Error> {
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
      Ok(l) => l,
      Err(e) => {
        error!("Failed to start daemon (is it already running?)");
        return Err(e);
      }
    };

    info!("Daemon listening on 127.0.0.1:{}", port);

    let (incoming_tx, incoming_rx) = broadcast::channel(100);
    let (outgoing_tx, outgoing_rx) = mpsc::channel(100);

    let daemon = Arc::new(Self {
      listener,
      incoming_tx,
      incoming_rx,
      outgoing_tx,
      outgoing_rx: Arc::new(Mutex::new(outgoing_rx)),
    });

    Ok(daemon)
  }

  /// Start accepting and handling incoming connections
  pub async fn start(&self) {
    loop {
      match self.listener.accept().await {
        Ok((stream, addr)) => {
          info!("New connection from {}", addr);
          let incoming_tx = self.incoming_tx.clone();
          let outgoing_rx = self.outgoing_rx.clone();

          tokio::spawn(async move { Self::handle(stream, incoming_tx, outgoing_rx).await });
        }
        Err(e) => {
          error!("Failed to accept connection: {}", e);
          time::sleep(Duration::from_secs(1)).await;
        }
      }
    }
  }

  /// Handle a connection from a client
  /// Currently the same queue is used for every client, but it is indented to
  /// be used with one client so it is fine for now
  /// ### Arguments:
  /// * `stream` - The [`TcpStream`] for the client connect
  /// * `incoming_tx` - A [`broadcast::Sender<Packet>`]. Packets received from
  ///   clients are sent on this channel
  /// * `outgoing_rx` - A [`Arc<Mutex<mpsc::Receiver<Packet>>>`]. Packets that
  ///   need to be send to the client are received
  /// on this channel
  // TODO: Possibly use self in the future
  async fn handle(
    stream: TcpStream,
    incoming_tx: broadcast::Sender<Packet>,
    outgoing_rx: Arc<Mutex<mpsc::Receiver<Packet>>>,
  ) {
    let (mut read_half, mut write_half) = stream.into_split();

    let (shutdown_tx, mut shutdown_rx) = watch::channel(());
    let mut outgoing_lock = outgoing_rx.lock().await;

    loop {
      tokio::select! {
        res = read_half.read_u16() => {
          match res {
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

                if let Err(e) = incoming_tx.send(packet) {
                  error!("Failed to add send packet (local): {}", e);
                };
              }
              Err(e) => {
                error!("Read error: {}", e);
                break;
              }
            }
          }
          Err(_) => {
              info!("Connection closed by peer");
              let _ = shutdown_tx.send(());
              break;
            }
          }
        }

        res = outgoing_lock.recv() => {
          match res {
            Some(p) => {
              let bytes = Packet::to_bytes(&p).unwrap();
              debug!("Sending packet: {:?}", p);
              debug!("Raw packet: {:?}", bytes);
              if let Err(e) = write_half.write_all(&bytes).await {
                error!("Write error: {}", e);
              }

              let _ = write_half.flush().await;
            },
            None => todo!(),
          }
        }

        _ = shutdown_rx.changed() => {
          break;
        }
      }
    }
  }

  /// Add a packet to the queue.
  ///
  /// Arguments:
  /// * `packet` - A [`Packet`] to be sent (will be sent to all connected
  ///   clients)
  pub async fn send_packet(&self, packet: Packet) {
    let _ = self.outgoing_tx.send(packet).await;
  }

  /// Get a stream of incoming packets
  pub fn packet_stream(&self) -> impl tokio_stream::Stream<Item = Result<Packet, BroadcastStreamRecvError>> {
    BroadcastStream::new(self.incoming_rx.resubscribe())
  }
}
