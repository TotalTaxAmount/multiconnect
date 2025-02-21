use async_trait::async_trait;
use libp2p::{
  futures::{io, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
  request_response,
};
use multiconnect_protocol::peer::{PeerPairRequest, PeerPairResponse};

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
  #[must_use]
  async fn read_request<T: AsyncRead + Unpin + Send>(
    &mut self,
    protocol: &Self::Protocol,
    io: &mut T,
  ) -> io::Result<Self::Request> {
    // let mut len_buf = [0u8; 4];
    // io.read_exact(&mut len_buf).await?;
    // let len = u32::from_be_bytes(len_buf) as usize;

    let mut buf = Vec::new();
    io.read_exact(&mut buf).await?;

    PeerPairRequest::decode(&*buf).map_err(|e| io::Error::new(std::io::ErrorKind::InvalidData, e))
  }

  #[doc = " Reads a response from the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  #[must_use]
  async fn read_response<T: AsyncRead + Unpin + Send>(
    &mut self,
    protocol: &Self::Protocol,
    io: &mut T,
  ) -> io::Result<Self::Response> {
    // let mut len_buf = [0u8; 4];
    // io.read_exact(&mut len_buf).await?;
    // let len = u32::from_be_bytes(len_buf) as usize;

    let mut buf = Vec::new();
    io.read_exact(&mut buf).await?;

    PeerPairResponse::decode(&*buf).map_err(|e| io::Error::new(std::io::ErrorKind::InvalidData, e))
  }

  #[doc = " Writes a request to the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  #[must_use]
  async fn write_request<T: AsyncWrite + Unpin + Send>(
    &mut self,
    protocol: &Self::Protocol,
    io: &mut T,
    req: Self::Request,
  ) -> io::Result<()> {
    let mut buf = Vec::new();
    req.encode(&mut buf).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    io.write_all(&buf).await?;
    io.flush().await
  }

  #[doc = " Writes a response to the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  #[must_use]
  async fn write_response<T: AsyncWrite + Unpin + Send>(
    &mut self,
    protocol: &Self::Protocol,
    io: &mut T,
    res: Self::Response,
  ) -> io::Result<()> {
    let mut buf = Vec::new();
    res.encode(&mut buf).map_err(|e| io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    io.write_all(&buf).await?;
    io.flush().await
  }
}
