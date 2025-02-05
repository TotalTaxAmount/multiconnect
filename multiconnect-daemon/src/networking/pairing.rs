use async_trait::async_trait;
use libp2p::{
  futures::{io, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
  request_response,
};

#[derive(Clone, Default)]
pub(crate) struct PairingCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairingRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairingResponse(pub bool);

#[async_trait]
impl request_response::Codec for PairingCodec {
  #[doc = " The type of protocol(s) or protocol versions being negotiated."]
  type Protocol = String;

  #[doc = " The type of inbound and outbound requests."]
  type Request = PairingRequest;

  #[doc = " The type of inbound and outbound responses."]
  type Response = PairingResponse;

  #[doc = " Reads a request from the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  #[must_use]
  async fn read_request<T: AsyncRead + Unpin + Send>(
    &mut self,
    protocol: &Self::Protocol,
    io: &mut T,
  ) -> io::Result<Self::Request> {
    let mut buf = [0u8; 1];
    io.read_exact(&mut buf).await?;
    Ok(PairingRequest)
  }

  #[doc = " Reads a response from the given I/O stream according to the"]
  #[doc = " negotiated protocol."]
  #[must_use]
  async fn read_response<T: AsyncRead + Unpin + Send>(
    &mut self,
    protocol: &Self::Protocol,
    io: &mut T,
  ) -> io::Result<Self::Response> {
    let mut buf = [0u8; 1];
    io.read_exact(&mut buf).await?;
    Ok(PairingResponse(buf[0] != 0))
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
    io.write_all(&[1]).await?;
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
    io.write_all(&[res.0 as u8]).await?;
    io.flush().await
  }
}
