//! Strict bounded request-response stream codec.

use std::io;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::StreamProtocol;
use libp2p::request_response::Codec;
use protocol::{
    MAX_ACK_BODY_BYTES, MAX_OFFER_BODY_BYTES, MAX_OPEN_BODY_BYTES, TRANSFER_PROTOCOL,
    TransferRequest, TransferResponse, decode_request_frame, decode_response_frame,
    encode_request_frame, encode_response_frame, parse_frame_length,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct TransferCodec;

pub(crate) fn transfer_protocol() -> StreamProtocol {
    StreamProtocol::new(TRANSFER_PROTOCOL)
}

#[async_trait]
impl Codec for TransferCodec {
    type Protocol = StreamProtocol;
    type Request = TransferRequest;
    type Response = TransferResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let frame = read_frame(io, MAX_OPEN_BODY_BYTES.max(MAX_ACK_BODY_BYTES)).await?;
        decode_request_frame(&frame).map_err(invalid_data)
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let frame = read_frame(io, MAX_OFFER_BODY_BYTES).await?;
        decode_response_frame(&frame).map_err(invalid_data)
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        request: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let frame = encode_request_frame(&request).map_err(invalid_data)?;
        write_frame(io, &frame).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        response: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let frame = encode_response_frame(&response).map_err(invalid_data)?;
        write_frame(io, &frame).await
    }
}

async fn read_frame<T>(io: &mut T, limit: usize) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin,
{
    let mut header = [0_u8; 4];
    io.read_exact(&mut header).await?;
    let length = parse_frame_length(&header).map_err(invalid_data)?;
    if length > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds protocol limit",
        ));
    }
    let mut frame = Vec::with_capacity(4 + length);
    frame.extend_from_slice(&header);
    frame.resize(4 + length, 0);
    io.read_exact(&mut frame[4..]).await?;
    Ok(frame)
}

async fn write_frame<T>(io: &mut T, frame: &[u8]) -> io::Result<()>
where
    T: AsyncWrite + Unpin,
{
    io.write_all(frame).await?;
    io.close().await
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
