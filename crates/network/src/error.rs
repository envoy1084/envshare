//! Secret-safe network errors.

/// Failures exposed by the bounded networking API.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NetworkError {
    /// The network task is no longer running.
    #[error("network task stopped")]
    TaskStopped,
    /// A bounded command or event queue is saturated.
    #[error("network task is busy")]
    Busy,
    /// A listen address was invalid or rejected.
    #[error("listen operation failed")]
    Listen,
    /// A dial could not be started or completed.
    #[error("dial operation failed")]
    Dial,
    /// A request failed before a valid response arrived.
    #[error("request operation failed")]
    Request,
    /// A response could not be delivered to its inbound stream.
    #[error("response operation failed")]
    Response,
    /// Transport or behavior construction failed.
    #[error("network configuration failed")]
    Configuration,
}
