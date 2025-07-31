use std::sync::Arc;

use log::{debug, error, info, trace};
use multiconnect_protocol::Packet;
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpSocket,
  sync::{
    broadcast::{self},
    mpsc,
  },
};

pub type SharedDaemon = Arc<Daemon>;

#[derive(Debug)]
pub struct Daemon {
  /// Incoming packet receiver from daemon
  incoming_rx: broadcast::Receiver<Packet>,

  /// Outgoing packet sender to daemon
  outgoing_tx: mpsc::Sender<Packet>,
}

impl Daemon {
  /// Connect to the daemon and establish a [`TcpStream`]
  pub async fn connect(port: &u16) -> Result<SharedDaemon, Box<dyn std::error::Error>> {
    let socket = TcpSocket::new_v4()?;
    let stream = match socket.connect(format!("127.0.0.1:{}", port).parse()?).await {
      Ok(s) => s,
      Err(e) => {
        error!("Failed to connect to daemon: {}", e);
        return Err(Box::new(e));
      }
    };
    info!("Connected to the daemon");

    let (incoming_tx, incoming_rx) = broadcast::channel(100);
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel(100);

    tokio::spawn(async move {
      let (mut read_half, mut write_half) = stream.into_split();

      loop {
        tokio::select! {
          res = read_half.read_u16() => {
            match res {
              Ok(len) => {
                if len > u16::MAX {
                  error!("Packet is to big: {}", len);
                  continue;
                }

                trace!("Received packet with len {}", len);

                let mut raw: Vec<u8> = vec![0u8; len.into()];
                match read_half.read_exact(&mut raw).await {
                  Ok(_) => {
                    let packet = match Packet::from_bytes(&raw) {
                      Ok(p) => p,
                      Err(e) => {
                        error!("Error decoding packet {}", e);
                        continue;
                      }
                    };


                    trace!("Received {:?} packet", packet);

                    if let Err(e) = incoming_tx.send(packet) {
                      error!("Error sending packet (local): {}", e);
                    }
                  }
                  Err(e) => {
                    error!("Read error: {}", e);
                    continue;
                  }
                };
              }
              Err(_) => {
                info!("Connection closed by peer");
                break;
              }
            }
          }
          res = outgoing_rx.recv() => {
            match res {
              Some(p) => {
                let bytes = Packet::to_bytes(&p).unwrap();
                debug!("Sending packet: {:?}", p);
                debug!("Raw packet: {:?}", bytes);
                if let Err(e) = write_half.write_all(&bytes).await {
                  error!("Write error: {}", e);
                }

                let _ = write_half.flush().await;
              }
              None => todo!(),
            }
          }
        }
      }
    });

    Ok(Arc::new(Self { incoming_rx, outgoing_tx }))
  }

  /// Send a packet
  pub fn sending_stream(&self) -> mpsc::Sender<Packet> {
    self.outgoing_tx.clone()
  }

  /// Get a stream of incoming packets
  pub fn packet_stream(&self) -> broadcast::Receiver<Packet> {
    self.incoming_rx.resubscribe()
  }
}
