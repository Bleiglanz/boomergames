//! Offline Step 1 transport adapter running game sessions locally in WASM.

use std::collections::VecDeque;

use boomer_domain::{GameKind, LocalMode};
use boomer_protocol::{ClientMessage, ErrorCode, PROTOCOL_VERSION, ServerMessage, SessionKindDto};

use crate::mapping::{
    game_kind_from_dto, game_kind_to_dto, local_mode_from_dto, local_mode_to_dto,
};
use crate::ports::{GameTransport, TransportError};

/// Offline [`GameTransport`] adapter for single-player Hotseat, vs. AI, and Free Practice.
#[derive(Debug, Default)]
pub struct LocalSessionAdapter {
    handshaken: bool,
    active_game: Option<GameKind>,
    active_mode: Option<LocalMode>,
    outbox: VecDeque<ServerMessage>,
}

impl LocalSessionAdapter {
    /// Creates a new offline session adapter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl GameTransport for LocalSessionAdapter {
    fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        match msg {
            ClientMessage::Hello { v, .. } => {
                if v != PROTOCOL_VERSION {
                    self.outbox.push_back(ServerMessage::Error {
                        code: ErrorCode::UpgradeRequired,
                        re: "hello".to_owned(),
                    });
                    return Ok(());
                }
                self.handshaken = true;
                self.outbox.push_back(ServerMessage::Welcome {
                    protocol_version: PROTOCOL_VERSION,
                    mode: SessionKindDto::Local,
                });
                self.outbox.push_back(ServerMessage::SessionState {
                    game: None,
                    mode: None,
                    status: "home_ready".to_owned(),
                });
                Ok(())
            }
            ClientMessage::LocalStart { game, mode } => {
                if !self.handshaken {
                    self.outbox.push_back(ServerMessage::Error {
                        code: ErrorCode::NotAllowed,
                        re: "local.start".to_owned(),
                    });
                    return Ok(());
                }
                let domain_game = game_kind_from_dto(game);
                let domain_mode = local_mode_from_dto(mode);
                if domain_game == GameKind::Checkers && domain_mode == LocalMode::FreePractice {
                    self.outbox.push_back(ServerMessage::Error {
                        code: ErrorCode::Invalid,
                        re: "local.start".to_owned(),
                    });
                    return Ok(());
                }
                self.active_game = Some(domain_game);
                self.active_mode = Some(domain_mode);
                let status = match domain_game {
                    GameKind::Checkers => "checkers_shell_ready",
                    GameKind::Pool => "pool_shell_ready",
                };
                self.outbox.push_back(ServerMessage::SessionState {
                    game: Some(game_kind_to_dto(domain_game)),
                    mode: Some(local_mode_to_dto(domain_mode)),
                    status: status.to_owned(),
                });
                Ok(())
            }
            ClientMessage::LocalLeave {} => {
                self.active_game = None;
                self.active_mode = None;
                self.outbox.push_back(ServerMessage::SessionState {
                    game: None,
                    mode: None,
                    status: "home_ready".to_owned(),
                });
                Ok(())
            }
        }
    }

    fn poll_messages(&mut self) -> Vec<ServerMessage> {
        self.outbox.drain(..).collect()
    }

    fn tick(&mut self, _dt_ms: u32) {
        // Milestones 1 & 2 will step clocks and time-sliced AI search here.
    }
}
