use async_trait::async_trait;
use libp2p::{
  futures::{io, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
  request_response,
};

use crate::Packet;
use log::debug;

#[derive(Clone, Default)]
pub struct PacketCodec;

#[async_trait]
impl request_response::Codec for PacketCodec {
  #[doc = " The type of protocol(s) or protocol versions being negotiated."]
  type Protocol = String;

  #[doc = " The type of inbound and outbound requests."]
  type Request = Packet;

  #[doc = " The type of inbound and outbound responses."]
  type Response = ();

  #[doc = " Reads a request from the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn read_request<T>(&mut self, protocol: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
  where
    T: AsyncRead + Unpin + Send,
  {
    debug!("Read request");
    let mut len_buf = [0u8; 2];
    io.read_exact(&mut len_buf).await?;
    let len = u16::from_be_bytes(len_buf) as usize;
    debug!("Len: {}", len);

    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;
    debug!("Raw packet: {:?}", buf);

    Packet::from_bytes(&buf).map_err(|e| io::Error::new(std::io::ErrorKind::InvalidData, e))
  }

  #[doc = " Reads a response from the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn read_response<T>(&mut self, _protocol: &Self::Protocol, _io: &mut T) -> io::Result<Self::Response>
  where
    T: AsyncRead + Unpin + Send,
  {
    Ok(())
  }

  #[doc = " Writes a request to the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn write_request<T>(&mut self, _protocol: &Self::Protocol, io: &mut T, req: Self::Request) -> io::Result<()>
  where
    T: AsyncWrite + Unpin + Send,
  {
    debug!("Write request");
    let bytes: Vec<u8> = Packet::to_bytes(&req).map_err(|e| io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    debug!("Raw bytes: {:?}", bytes);
    io.write_all(&bytes).await?;
    io.flush().await
  }

  #[doc = " Writes a response to the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn write_response<T>(&mut self, _protocol: &Self::Protocol, io: &mut T, _res: Self::Response) -> io::Result<()>
  where
    T: AsyncWrite + Unpin + Send,
  {
    io.flush().await
  }
}
