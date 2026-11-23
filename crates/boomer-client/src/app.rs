//! Client application state machine projecting `ViewModel` snapshots for the DOM/Three.js shell.

use boomer_protocol::{
    ClientMessage, ErrorCode, GameKindDto, LocalModeDto, PROTOCOL_VERSION, ServerMessage,
    SessionKindDto,
};
use serde::{Deserialize, Serialize};

use crate::animation::{POOL_FRAME_LEN, PoolAnimationBuffer};
use crate::ports::{GameTransport, TransportError};

/// Discrete UI inputs sent from the thin TypeScript layer into WASM.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiInput {
    /// Start an offline game session from the home picker.
    StartLocalGame {
        /// Selected game.
        game: GameKindDto,
        /// Selected offline mode.
        mode: LocalModeDto,
    },
    /// Exit the current game shell back to the home picker.
    ReturnHome,
    /// Toggle WCAG high-contrast theme.
    ToggleHighContrast,
    /// Switch UI language (`"en"` or `"de"`).
    SetLanguage {
        /// Two-letter language code.
        lang: String,
    },
}

/// Active screen projected to the UI shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScreenView {
    /// Home game-picker screen.
    Home,
    /// Active game shell screen (canvas + side panel).
    GameShell {
        /// Active game.
        game: GameKindDto,
        /// Active local mode.
        mode: LocalModeDto,
        /// Status token translated by the shell i18n table.
        status: String,
    },
}

/// Complete immutable UI projection serialized to JS only when state changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewModel {
    /// Crate version string.
    pub version: String,
    /// Active transport mode (`local` in Step 1, `remote` in Step 2).
    pub session_kind: SessionKindDto,
    /// Current screen state.
    pub screen: ScreenView,
    /// High-contrast accessibility flag.
    pub high_contrast: bool,
    /// Active language code (`"en"` or `"de"`).
    pub lang: String,
    /// Latest user-facing error code, if any.
    pub last_error: Option<ErrorCode>,
}

/// Transport-agnostic client state machine.
pub struct ClientApp<T: GameTransport> {
    transport: T,
    view: ViewModel,
    pool_anim: PoolAnimationBuffer,
    dirty: bool,
}

impl<T: GameTransport> ClientApp<T> {
    /// Creates a new `ClientApp` and initiates the `hello` handshake on `transport`.
    #[must_use]
    pub fn new(mut transport: T) -> Self {
        let mut last_error = None;
        if transport
            .send(ClientMessage::Hello {
                v: PROTOCOL_VERSION,
                session: None,
                guest: None,
            })
            .is_err()
        {
            last_error = Some(ErrorCode::Invalid);
        }

        let mut app = Self {
            transport,
            view: ViewModel {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                session_kind: SessionKindDto::Local,
                screen: ScreenView::Home,
                high_contrast: false,
                lang: "en".to_owned(),
                last_error,
            },
            pool_anim: PoolAnimationBuffer::new_preview_rack(),
            dirty: true,
        };
        app.drain_transport();
        app
    }

    /// Handles a discrete [`UiInput`]. Returns `true` if the `ViewModel` changed.
    pub fn handle_input(&mut self, input: UiInput) -> bool {
        self.view.last_error = None;
        match input {
            UiInput::StartLocalGame { game, mode } => {
                if let Err(err) = self
                    .transport
                    .send(ClientMessage::LocalStart { game, mode })
                {
                    self.record_transport_error(&err);
                }
            }
            UiInput::ReturnHome => {
                if let Err(err) = self.transport.send(ClientMessage::LocalLeave {}) {
                    self.record_transport_error(&err);
                }
            }
            UiInput::ToggleHighContrast => {
                self.view.high_contrast = !self.view.high_contrast;
                self.dirty = true;
            }
            UiInput::SetLanguage { lang } => {
                if lang == "en" || lang == "de" {
                    self.view.lang = lang;
                    self.dirty = true;
                } else {
                    self.view.last_error = Some(ErrorCode::Invalid);
                    self.dirty = true;
                }
            }
        }
        self.drain_transport();
        self.take_dirty()
    }

    /// Advances the client transport by `dt_ms`. Returns `true` if `ViewModel` changed.
    pub fn tick(&mut self, dt_ms: u32) -> bool {
        self.transport.tick(dt_ms);
        self.drain_transport();
        self.take_dirty()
    }

    /// Returns a reference to the current [`ViewModel`].
    #[must_use]
    pub const fn view_model(&self) -> &ViewModel {
        &self.view
    }

    /// Samples the 60 fps Pool ball frame buffer at `t_ms`.
    #[must_use]
    pub fn pool_frame(&self, t_ms: f64) -> &[f64; POOL_FRAME_LEN] {
        self.pool_anim.sample(t_ms)
    }

    fn drain_transport(&mut self) {
        for msg in self.transport.poll_messages() {
            match msg {
                ServerMessage::Welcome { mode, .. } => {
                    self.view.session_kind = mode;
                    self.dirty = true;
                }
                ServerMessage::SessionState { game, mode, status } => {
                    self.view.screen = match (game, mode) {
                        (Some(game), Some(mode)) => ScreenView::GameShell { game, mode, status },
                        _ => ScreenView::Home,
                    };
                    self.dirty = true;
                }
                ServerMessage::Error { code, .. } => {
                    self.view.last_error = Some(code);
                    self.dirty = true;
                }
            }
        }
    }

    fn record_transport_error(&mut self, _err: &TransportError) {
        self.view.last_error = Some(ErrorCode::Invalid);
        self.dirty = true;
    }

    const fn take_dirty(&mut self) -> bool {
        let was_dirty = self.dirty;
        self.dirty = false;
        was_dirty
    }
}

#[cfg(test)]
mod tests {
    use boomer_protocol::BotLevelDto;

    use super::*;
    use crate::local_adapter::LocalSessionAdapter;

    #[test]
    fn offline_lifecycle_transitions_between_home_and_games() {
        let mut app = ClientApp::new(LocalSessionAdapter::new());
        assert_eq!(app.view_model().screen, ScreenView::Home);
        assert_eq!(app.view_model().session_kind, SessionKindDto::Local);

        let changed = app.handle_input(UiInput::StartLocalGame {
            game: GameKindDto::Checkers,
            mode: LocalModeDto::VsBot {
                level: BotLevelDto::Medium,
            },
        });
        assert!(changed);
        assert_eq!(
            app.view_model().screen,
            ScreenView::GameShell {
                game: GameKindDto::Checkers,
                mode: LocalModeDto::VsBot {
                    level: BotLevelDto::Medium,
                },
                status: "checkers_shell_ready".to_owned(),
            }
        );

        let changed = app.handle_input(UiInput::ReturnHome);
        assert!(changed);
        assert_eq!(app.view_model().screen, ScreenView::Home);
    }

    #[test]
    fn checkers_rejects_free_practice_mode() {
        let mut app = ClientApp::new(LocalSessionAdapter::new());
        let changed = app.handle_input(UiInput::StartLocalGame {
            game: GameKindDto::Checkers,
            mode: LocalModeDto::FreePractice,
        });
        assert!(changed);
        assert_eq!(app.view_model().screen, ScreenView::Home);
        assert_eq!(app.view_model().last_error, Some(ErrorCode::Invalid));
    }

    #[test]
    fn pool_accepts_free_practice_and_exposes_frame_buffer() {
        let mut app = ClientApp::new(LocalSessionAdapter::new());
        let _ = app.handle_input(UiInput::StartLocalGame {
            game: GameKindDto::Pool,
            mode: LocalModeDto::FreePractice,
        });
        assert_eq!(
            app.view_model().screen,
            ScreenView::GameShell {
                game: GameKindDto::Pool,
                mode: LocalModeDto::FreePractice,
                status: "pool_shell_ready".to_owned(),
            }
        );
        assert_eq!(app.pool_frame(0.0).len(), POOL_FRAME_LEN);
    }
}
