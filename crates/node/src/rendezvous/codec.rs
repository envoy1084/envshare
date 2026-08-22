//! Strict, bounded wire compatibility for libp2p Rendezvous v1.

use std::io;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use libp2p::swarm::StreamProtocol;
use prost::Message as _;

const MAX_WIRE_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub(crate) enum Request {
    Register {
        namespace: String,
        signed_record: Vec<u8>,
        ttl: Option<u64>,
    },
    Unregister {
        namespace: String,
    },
    Discover {
        namespace: Option<String>,
        cookie: Option<Vec<u8>>,
        limit: Option<u64>,
    },
}

#[derive(Clone)]
pub(crate) enum Response {
    Register(Result<u64, Status>),
    Discover(Result<DiscoveryPage, Status>),
}

#[derive(Clone)]
pub(crate) struct DiscoveryPage {
    pub registrations: Vec<WireRegistration>,
    pub cookie: Vec<u8>,
}

#[derive(Clone)]
pub(crate) struct WireRegistration {
    pub namespace: String,
    pub signed_record: Vec<u8>,
    pub ttl: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub(crate) enum Status {
    InvalidNamespace = 100,
    InvalidSignedPeerRecord = 101,
    InvalidTtl = 102,
    InvalidCookie = 103,
    NotAuthorized = 200,
    Unavailable = 400,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Codec;

#[async_trait]
impl libp2p::request_response::Codec for Codec {
    type Protocol = StreamProtocol;
    type Request = Request;
    type Response = Response;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let bytes = read_frame(io).await?;
        let message = WireMessage::decode(bytes.as_slice()).map_err(invalid_data)?;
        Request::try_from(message)
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        _io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "inbound-only protocol received a response",
        ))
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        _io: &mut T,
        _request: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "inbound-only protocol cannot write a request",
        ))
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
        let message = WireMessage::from(response);
        let length = message.encoded_len();
        if length > MAX_WIRE_MESSAGE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "rendezvous response exceeds its bound",
            ));
        }
        let mut payload = Vec::with_capacity(length);
        message.encode(&mut payload).map_err(invalid_data)?;
        let prefix = encode_varint(length)?;
        io.write_all(&prefix).await?;
        io.write_all(&payload).await?;
        io.flush().await
    }
}

async fn read_frame<T>(io: &mut T) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin + Send,
{
    let mut length = 0_u64;
    for shift in (0..=63).step_by(7) {
        let mut input = [0_u8; 1];
        io.read_exact(&mut input).await?;
        let byte = input[0];
        if shift == 63 && byte > 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid rendezvous frame length",
            ));
        }
        length |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            let length = usize::try_from(length).map_err(invalid_data)?;
            if length > MAX_WIRE_MESSAGE_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "rendezvous request exceeds its bound",
                ));
            }
            let mut payload = vec![0_u8; length];
            io.read_exact(&mut payload).await?;
            return Ok(payload);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid rendezvous frame length",
    ))
}

fn encode_varint(length: usize) -> io::Result<Vec<u8>> {
    let mut value = u64::try_from(length).map_err(invalid_data)?;
    let mut output = Vec::with_capacity(4);
    loop {
        let mut byte = u8::try_from(value & 0x7f).map_err(invalid_data)?;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return Ok(output);
        }
    }
}

impl TryFrom<WireMessage> for Request {
    type Error = io::Error;

    fn try_from(message: WireMessage) -> Result<Self, Self::Error> {
        match (
            message.message_type,
            message.register,
            message.unregister,
            message.discover,
        ) {
            (Some(0), Some(register), _, _) => Ok(Self::Register {
                namespace: register.namespace.ok_or_else(missing_field)?,
                signed_record: register.signed_peer_record.ok_or_else(missing_field)?,
                ttl: register.ttl,
            }),
            (Some(2), _, Some(unregister), _) => Ok(Self::Unregister {
                namespace: unregister.namespace.ok_or_else(missing_field)?,
            }),
            (Some(3), _, _, Some(discover)) => Ok(Self::Discover {
                namespace: discover.namespace,
                cookie: discover.cookie,
                limit: discover.limit,
            }),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "inconsistent rendezvous request",
            )),
        }
    }
}

impl From<Response> for WireMessage {
    fn from(response: Response) -> Self {
        match response {
            Response::Register(result) => {
                let (status, ttl) = result.map_or_else(
                    |status| (status as i32, None),
                    |ttl| (ResponseStatus::Ok as i32, Some(ttl)),
                );
                Self {
                    message_type: Some(MessageType::RegisterResponse as i32),
                    register_response: Some(WireRegisterResponse {
                        status: Some(status),
                        ttl,
                    }),
                    ..Self::default()
                }
            }
            Response::Discover(result) => {
                let response = result.map_or_else(
                    |status| WireDiscoverResponse {
                        status: Some(status as i32),
                        ..WireDiscoverResponse::default()
                    },
                    |page| WireDiscoverResponse {
                        registrations: page
                            .registrations
                            .into_iter()
                            .map(|registration| WireRegister {
                                namespace: Some(registration.namespace),
                                signed_peer_record: Some(registration.signed_record),
                                ttl: Some(registration.ttl),
                            })
                            .collect(),
                        cookie: Some(page.cookie),
                        status: Some(ResponseStatus::Ok as i32),
                    },
                );
                Self {
                    message_type: Some(MessageType::DiscoverResponse as i32),
                    discover_response: Some(response),
                    ..Self::default()
                }
            }
        }
    }
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

fn missing_field() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "missing rendezvous field")
}

impl std::fmt::Debug for Request {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Register {
                signed_record, ttl, ..
            } => formatter
                .debug_struct("Register")
                .field("namespace", &"[REDACTED]")
                .field("signed_record_bytes", &signed_record.len())
                .field("ttl", ttl)
                .finish(),
            Self::Unregister { .. } => formatter.write_str("Unregister([REDACTED])"),
            Self::Discover { cookie, limit, .. } => formatter
                .debug_struct("Discover")
                .field("namespace", &"[REDACTED]")
                .field("cookie_present", &cookie.is_some())
                .field("limit", limit)
                .finish(),
        }
    }
}

impl std::fmt::Debug for Response {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Register(result) => formatter.debug_tuple("Register").field(result).finish(),
            Self::Discover(Ok(page)) => formatter
                .debug_struct("Discover")
                .field("registration_count", &page.registrations.len())
                .finish(),
            Self::Discover(Err(status)) => formatter.debug_tuple("Discover").field(status).finish(),
        }
    }
}

impl std::fmt::Debug for WireRegistration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WireRegistration")
            .field("namespace", &"[REDACTED]")
            .field("signed_record_bytes", &self.signed_record.len())
            .field("ttl", &self.ttl)
            .finish()
    }
}

#[derive(Clone, PartialEq, prost::Message)]
struct WireMessage {
    #[prost(enumeration = "MessageType", optional, tag = "1")]
    message_type: Option<i32>,
    #[prost(message, optional, tag = "2")]
    register: Option<WireRegister>,
    #[prost(message, optional, tag = "3")]
    register_response: Option<WireRegisterResponse>,
    #[prost(message, optional, tag = "4")]
    unregister: Option<WireUnregister>,
    #[prost(message, optional, tag = "5")]
    discover: Option<WireDiscover>,
    #[prost(message, optional, tag = "6")]
    discover_response: Option<WireDiscoverResponse>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct WireRegister {
    #[prost(string, optional, tag = "1")]
    namespace: Option<String>,
    #[prost(bytes = "vec", optional, tag = "2")]
    signed_peer_record: Option<Vec<u8>>,
    #[prost(uint64, optional, tag = "3")]
    ttl: Option<u64>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct WireRegisterResponse {
    #[prost(enumeration = "ResponseStatus", optional, tag = "1")]
    status: Option<i32>,
    #[prost(uint64, optional, tag = "3")]
    ttl: Option<u64>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct WireUnregister {
    #[prost(string, optional, tag = "1")]
    namespace: Option<String>,
    #[prost(bytes = "vec", optional, tag = "2")]
    id: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct WireDiscover {
    #[prost(string, optional, tag = "1")]
    namespace: Option<String>,
    #[prost(uint64, optional, tag = "2")]
    limit: Option<u64>,
    #[prost(bytes = "vec", optional, tag = "3")]
    cookie: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct WireDiscoverResponse {
    #[prost(message, repeated, tag = "1")]
    registrations: Vec<WireRegister>,
    #[prost(bytes = "vec", optional, tag = "2")]
    cookie: Option<Vec<u8>>,
    #[prost(enumeration = "ResponseStatus", optional, tag = "3")]
    status: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, prost::Enumeration)]
enum MessageType {
    Register = 0,
    RegisterResponse = 1,
    Unregister = 2,
    Discover = 3,
    DiscoverResponse = 4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, prost::Enumeration)]
enum ResponseStatus {
    Ok = 0,
    InvalidNamespace = 100,
    InvalidSignedPeerRecord = 101,
    InvalidTtl = 102,
    InvalidCookie = 103,
    NotAuthorized = 200,
    InternalError = 300,
    Unavailable = 400,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_response_uses_the_standard_wire_tags() -> Result<(), Box<dyn std::error::Error>> {
        let message = WireMessage::from(Response::Register(Ok(30)));
        let bytes = message.encode_to_vec();
        let decoded = WireMessage::decode(bytes.as_slice())?;

        assert_eq!(decoded.message_type, Some(1));
        assert_eq!(
            decoded.register_response.and_then(|value| value.ttl),
            Some(30)
        );
        Ok(())
    }

    #[test]
    fn request_debug_redacts_namespace_and_record() {
        let request = Request::Register {
            namespace: "SECRET-NAMESPACE-SENTINEL".to_owned(),
            signed_record: b"SECRET-RECORD-SENTINEL".to_vec(),
            ttl: Some(30),
        };
        let debug = format!("{request:?}");

        assert!(!debug.contains("SECRET-NAMESPACE-SENTINEL"));
        assert!(!debug.contains("SECRET-RECORD-SENTINEL"));
    }
}
