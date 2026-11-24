//! In-memory message bridge [`GameTransport`] used by `BoomerRemoteClient` in Step 2
//! to feed raw WebSocket `ServerMessage`s into [`crate::app::ClientApp`] and drain outgoing
//! `ClientMessage`s back to the browser's `OnlineSession`.

use std::collections::VecDeque;

use boomer_protocol::{ClientMessage, ServerMessage};

use crate::ports::{GameTransport, TransportError};

/// Bidirectional queue transport bridging JS WebSocket frames with [`crate::app::ClientApp`].
#[derive(Debug, Default)]
pub struct BridgeTransport {
    inbound: VecDeque<ServerMessage>,
    outbound: VecDeque<ClientMessage>,
}

impl BridgeTransport {
    /// Creates an empty bridge transport.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueues a [`ServerMessage`] received from `boomer-server` for [`crate::app::ClientApp`] to consume.
    pub fn push_inbound(&mut self, msg: ServerMessage) {
        self.inbound.push_back(msg);
    }

    /// Drains all [`ClientMessage`]s produced by user interactions for transmission over `/ws`.
    pub fn drain_outbound(&mut self) -> Vec<ClientMessage> {
        self.outbound.drain(..).collect()
    }
}

impl GameTransport for BridgeTransport {
    fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        // Handshake and offline-only lifecycle messages are handled outside this game bridge.
        if !matches!(
            msg,
            ClientMessage::Hello { .. }
                | ClientMessage::LocalStart { .. }
                | ClientMessage::LocalLeave {}
        ) {
            self.outbound.push_back(msg);
        }
        Ok(())
    }

    fn poll_messages(&mut self) -> Vec<ServerMessage> {
        self.inbound.drain(..).collect()
    }

    fn tick(&mut self, _dt_ms: u32) {}
}
