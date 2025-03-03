use async_trait::async_trait;
use libp2p::{
  futures::{io, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
  request_response, StreamProtocol,
};

use multiconnect_protocol::{
  peer::{PeerPairRequest, PeerPairResponse},
  Packet,
};

use prost::Message;

#[derive(Clone, Default)]
pub(crate) struct PairingCodec;

#[async_trait]
impl request_response::Codec for PairingCodec {
  #[doc = " The type of protocol(s) or protocol versions being negotiated."]
  type Protocol = String;

  #[doc = " The type of inbound and outbound requests."]
  type Request = PeerPairRequest;

  #[doc = " The type of inbound and outbound responses."]
  type Response = PeerPairResponse;

  #[doc = " Reads a request from the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn read_request<T>(&mut self, protocol: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
  where
    T: AsyncRead + Unpin + Send,
  {
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;

    PeerPairRequest::decode(&buf[..]).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
  }

  #[doc = " Reads a response from the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn read_response<T>(&mut self, protocol: &Self::Protocol, io: &mut T) -> io::Result<Self::Response>
  where
    T: AsyncRead + Unpin + Send,
  {
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;

    PeerPairResponse::decode(&buf[..]).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
  }

  #[doc = " Writes a request to the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn write_request<T>(&mut self, protocol: &Self::Protocol, io: &mut T, req: Self::Request) -> io::Result<()>
  where
    T: AsyncWrite + Unpin + Send,
  {
    let mut buf: Vec<u8> = Vec::new();
    req.encode(&mut buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let len = buf.len() as u32;
    io.write_all(&len.to_be_bytes()).await?;

    io.write_all(&buf).await?;
    io.flush().await
  }

  #[doc = " Writes a response to the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  async fn write_response<T>(&mut self, protocol: &Self::Protocol, io: &mut T, res: Self::Response) -> io::Result<()>
  where
    T: AsyncWrite + Unpin + Send,
  {
    let mut buf = Vec::new();
    res.encode(&mut buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let len = buf.len() as u32;
    io.write_all(&len.to_be_bytes()).await?;

    io.write_all(&buf).await?;
    io.flush().await
  }
}
