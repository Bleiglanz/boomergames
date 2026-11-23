//! Clean Architecture port traits decoupling `ClientApp` from transport, time, and randomness.

use boomer_protocol::{ClientMessage, ServerMessage};

/// Errors returned by a `GameTransport` implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// Transport is not connected or ready.
    Disconnected,
    /// Message could not be accepted by the transport.
    InvalidMessage(String),
}

/// Bidirectional game transport port.
///
/// Step 1 implements this via [`crate::local_adapter::LocalSessionAdapter`] for
/// offline single-player play; Step 2 swaps in a WebSocket adapter with zero
/// changes to `ClientApp` or `boomer-domain`.
pub trait GameTransport {
    /// Sends a protocol message from the client to the session backend.
    ///
    /// # Errors
    /// Returns [`TransportError`] if the message cannot be delivered.
    fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError>;

    /// Drains all pending server/session messages produced since the last poll.
    fn poll_messages(&mut self) -> Vec<ServerMessage>;

    /// Advances transport timers and time-sliced offline AI search by `dt_ms`.
    fn tick(&mut self, dt_ms: u32);
}

/// Monotonic clock port for deterministic testing.
pub trait ClockPort {
    /// Returns elapsed milliseconds from an arbitrary monotonic origin.
    fn now_ms(&self) -> u64;
}

/// Randomness port for seeding local games cleanly outside `boomer-domain`.
pub trait RandomPort {
    /// Returns a 64-bit seed value.
    fn next_u64(&mut self) -> u64;
}
