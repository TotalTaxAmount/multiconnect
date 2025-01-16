use std::{collections::VecDeque, error::Error, sync::Arc};

use log::{debug, error, info};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::Mutex,
};

pub mod networking;

const PORT: u16 = 10999;

pub struct Daemon {
  listener: TcpListener,
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

    Ok(Self { listener })
  }

  pub async fn start(&self) -> Result<(), Box<dyn Error>> {
    while let Ok((stream, addr)) = self.listener.accept().await {
      info!("New connection from {}", addr);

      tokio::spawn(async move { Self::handle(stream).await });
    }

    Ok(())
  }

  async fn handle(stream: TcpStream) {
    let (mut read_half, mut write_half) = stream.into_split();

    // let queue_clone = queue.clone();
    let read_task = tokio::spawn(async move {
      loop {
        let mut raw: Vec<u8> = Vec::new();
        let mut buf = [0; 4096];
        while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
          let len = match read_half.read(&mut buf).await {
            Ok(0) => break,
            Ok(len) => len,
            Err(e) => {
              error!("Read error: {}", e);
              break;
            }
          };

          raw.extend_from_slice(&buf[..len]);
        }

        // let packet = ProtocolMessage::from_bytes(&raw).unwrap();
        // let mut queue_lock = queue_clone.lock().await;
        // match packet.msg_type {
        //   MsgType::Ping => {
        //     info!("Received Ping, Pong!");
        //     let packet = ProtocolMessage::new(MsgType::Acknowledge,
        // packet.id.to_be_bytes().to_vec());     queue_lock.
        // push_front(packet);   }
        //   MsgType::Acknowledge => todo!(),
        //   MsgType::TransferStart => todo!(),
        //   MsgType::TransferChunk => todo!(),
        //   MsgType::TransferEnd => todo!(),
        //   MsgType::Message => todo!(),
        //   MsgType::Status => todo!(),
        // };
      }
    });

    // let write_task = tokio::spawn(async move {
    //   loop {
    //     let mut queue_lock = queue.lock().await;
    //     if let Some(packet) = queue_lock.pop_back() {
    //       debug!("Sending {:?} packet", packet.msg_type);
    //       let data = packet.to_bytes();
    //       if let Err(e) = write_half.write_all(&data).await {
    //         error!("Write error: {}", e);
    //         break;
    //       }
    //     } else {
    //       tokio::task::yield_now().await;
    //     }
    //   }
    // });

    // let _ = tokio::try_join!(read_task, write_task);
  }
}
