//! Client application state machine projecting `ViewModel` snapshots for the DOM/Three.js shell.

use boomer_protocol::{
    CheckersSettingsDto, CheckersStateDto, ClientMessage, ErrorCode, GameKindDto, LocalModeDto,
    PROTOCOL_VERSION, ServerMessage, SessionKindDto,
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
    /// Click or keyboard-select a Checkers dark square (`1..=32`).
    CheckersSelectSquare {
        /// Standard `1..=32` dark-square notation.
        square: u8,
    },
    /// Drag-and-drop a Checkers piece from `from` to `to` (`1..=32`).
    CheckersDragMove {
        /// Starting dark square (`1..=32`).
        from: u8,
        /// Target dark square (`1..=32`).
        to: u8,
    },
    /// Clear the active Checkers piece/path selection.
    CheckersClearSelection,
    /// Flip the Checkers board perspective vertically.
    CheckersFlipBoard,
    /// Toggle display of standard `1..=32` square numbers on the Checkers board.
    CheckersToggleSquareNumbers,
    /// Update Checkers clock settings (starts a fresh game with the new clock).
    CheckersSetClock {
        /// Updated clock configuration.
        settings: CheckersSettingsDto,
    },
    /// Resign the active match.
    Resign,
    /// Offer a draw (Checkers).
    OfferDraw,
    /// Answer a pending draw offer (Checkers).
    AnswerDraw {
        /// `true` to accept, `false` to decline.
        accept: bool,
    },
    /// Request a rematch (alternates colors).
    Rematch,
}

/// Interactive Checkers UI projection derived strictly in Rust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckersView {
    /// Authoritative game snapshot from the transport.
    pub state: CheckersStateDto,
    /// Currently selected starting square and intermediate multi-jump hops (`1..=32`).
    pub selected_path: Vec<u8>,
    /// Dark squares (`1..=32`) whose pieces may legally move this turn.
    pub selectable_squares: Vec<u8>,
    /// Legal next-step or unambiguous final destination squares (`1..=32`) for `selected_path`.
    pub highlighted_targets: Vec<u8>,
    /// Whether the board is rendered flipped (White on top, Black on bottom).
    pub flipped: bool,
    /// Whether standard `1..=32` square numbers are overlaid on dark squares.
    pub show_square_numbers: bool,
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
        /// Checkers interactive state when `game == Checkers`.
        checkers: Option<Box<CheckersView>>,
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
    checkers_state: Option<CheckersStateDto>,
    selected_path: Vec<u8>,
    checkers_flipped: bool,
    show_square_numbers: bool,
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
            checkers_state: None,
            selected_path: Vec::new(),
            checkers_flipped: false,
            show_square_numbers: false,
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
                self.selected_path.clear();
                self.send_checked(ClientMessage::LocalStart { game, mode });
            }
            UiInput::ReturnHome => {
                self.selected_path.clear();
                self.checkers_state = None;
                self.send_checked(ClientMessage::LocalLeave {});
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
            UiInput::CheckersSelectSquare { square } => {
                self.handle_checkers_select(square);
            }
            UiInput::CheckersDragMove { from, to } => {
                self.handle_checkers_drag(from, to);
            }
            UiInput::CheckersClearSelection => {
                if !self.selected_path.is_empty() {
                    self.selected_path.clear();
                    self.sync_checkers_view();
                }
            }
            UiInput::CheckersFlipBoard => {
                self.checkers_flipped = !self.checkers_flipped;
                self.sync_checkers_view();
            }
            UiInput::CheckersToggleSquareNumbers => {
                self.show_square_numbers = !self.show_square_numbers;
                self.sync_checkers_view();
            }
            UiInput::CheckersSetClock { settings } => {
                self.selected_path.clear();
                self.send_checked(ClientMessage::CheckersSettings { settings });
            }
            UiInput::Resign => {
                self.selected_path.clear();
                self.send_checked(ClientMessage::Resign {});
            }
            UiInput::OfferDraw => {
                self.send_checked(ClientMessage::DrawOffer {});
            }
            UiInput::AnswerDraw { accept } => {
                self.send_checked(ClientMessage::DrawAnswer { accept });
            }
            UiInput::Rematch => {
                self.selected_path.clear();
                self.send_checked(ClientMessage::Rematch {});
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

    fn send_checked(&mut self, msg: ClientMessage) {
        if let Err(err) = self.transport.send(msg) {
            self.record_transport_error(&err);
        }
    }

    fn handle_checkers_select(&mut self, square: u8) {
        let Some(state) = self.checkers_state.clone() else {
            return;
        };
        if state.outcome.is_some() || state.bot_thinking {
            return;
        }

        // 1. If a selection is already active, check whether `square` completes or extends it.
        if !self.selected_path.is_empty() {
            if self.try_advance_or_submit(&state, square) {
                return;
            }
            // Clicking the currently selected root square toggles selection off.
            if self.selected_path.len() == 1 && self.selected_path.first() == Some(&square) {
                self.selected_path.clear();
                self.sync_checkers_view();
                return;
            }
        }

        // 2. Otherwise select `square` if any legal move starts there.
        let can_start = state
            .legal_moves
            .iter()
            .any(|mv| mv.path.first() == Some(&square));
        if can_start {
            self.selected_path = vec![square];
            self.sync_checkers_view();
        } else if !self.selected_path.is_empty() {
            self.selected_path.clear();
            self.sync_checkers_view();
        }
    }

    fn handle_checkers_drag(&mut self, from: u8, to: u8) {
        let Some(state) = self.checkers_state.clone() else {
            return;
        };
        if state.outcome.is_some() || state.bot_thinking {
            return;
        }
        // If `from` matches the current multi-jump tip, continue from `selected_path`.
        if self.selected_path.last() == Some(&from) && self.try_advance_or_submit(&state, to) {
            return;
        }
        let can_start = state
            .legal_moves
            .iter()
            .any(|mv| mv.path.first() == Some(&from));
        if !can_start {
            return;
        }
        self.selected_path = vec![from];
        if !self.try_advance_or_submit(&state, to) {
            self.selected_path.clear();
            self.sync_checkers_view();
        }
    }

    fn try_advance_or_submit(&mut self, state: &CheckersStateDto, target: u8) -> bool {
        let prefix = &self.selected_path;
        let matching: Vec<&boomer_protocol::CheckersMoveDto> = state
            .legal_moves
            .iter()
            .filter(|mv| mv.path.starts_with(prefix))
            .collect();

        // Case A: Exact step-by-step next hop.
        let mut candidate_prefix = prefix.clone();
        candidate_prefix.push(target);

        let exact_full = matching.iter().find(|mv| mv.path == candidate_prefix);
        if let Some(mv) = exact_full {
            let turn = state.turn_number;
            let path = mv.path.clone();
            self.selected_path.clear();
            self.send_checked(ClientMessage::CheckersMove { turn, path });
            return true;
        }

        let step_continuations = matching
            .iter()
            .any(|mv| mv.path.starts_with(&candidate_prefix));
        if step_continuations {
            self.selected_path = candidate_prefix;
            self.sync_checkers_view();
            return true;
        }

        // Case B: Unambiguous final destination of a multi-jump (plan.md C-9).
        let final_matches: Vec<&boomer_protocol::CheckersMoveDto> = matching
            .iter()
            .copied()
            .filter(|mv| mv.path.last() == Some(&target))
            .collect();
        if let [only_move] = final_matches.as_slice() {
            let turn = state.turn_number;
            let path = only_move.path.clone();
            self.selected_path.clear();
            self.send_checked(ClientMessage::CheckersMove { turn, path });
            return true;
        }

        false
    }

    fn build_checkers_view(&self, state: &CheckersStateDto) -> CheckersView {
        let mut selectable = Vec::new();
        for mv in &state.legal_moves {
            if let Some(&start) = mv.path.first()
                && !selectable.contains(&start)
            {
                selectable.push(start);
            }
        }

        let mut highlighted = Vec::new();
        if !self.selected_path.is_empty() {
            let matching: Vec<&boomer_protocol::CheckersMoveDto> = state
                .legal_moves
                .iter()
                .filter(|mv| mv.path.starts_with(&self.selected_path))
                .collect();

            // Immediate next hops
            let next_idx = self.selected_path.len();
            for mv in &matching {
                if let Some(&next_sq) = mv.path.get(next_idx)
                    && !highlighted.contains(&next_sq)
                {
                    highlighted.push(next_sq);
                }
            }

            // Unambiguous final squares of multi-jumps
            for mv in &matching {
                if let Some(&final_sq) = mv.path.last() {
                    let count = matching
                        .iter()
                        .filter(|other| other.path.last() == Some(&final_sq))
                        .count();
                    if count == 1 && !highlighted.contains(&final_sq) {
                        highlighted.push(final_sq);
                    }
                }
            }
        }

        CheckersView {
            state: state.clone(),
            selected_path: self.selected_path.clone(),
            selectable_squares: selectable,
            highlighted_targets: highlighted,
            flipped: self.checkers_flipped,
            show_square_numbers: self.show_square_numbers,
        }
    }

    fn sync_checkers_view(&mut self) {
        let next_view = self
            .checkers_state
            .as_ref()
            .map(|st| Box::new(self.build_checkers_view(st)));
        if let ScreenView::GameShell { checkers, .. } = &mut self.view.screen {
            *checkers = next_view;
            self.dirty = true;
        }
    }

    fn drain_transport(&mut self) {
        for msg in self.transport.poll_messages() {
            match msg {
                ServerMessage::Welcome { mode, .. } => {
                    self.view.session_kind = mode;
                    self.dirty = true;
                }
                ServerMessage::SessionState { game, mode, status } => {
                    self.view.screen = if let (Some(game), Some(mode)) = (game, mode) {
                        let checkers = if game == GameKindDto::Checkers {
                            self.checkers_state
                                .as_ref()
                                .map(|st| Box::new(self.build_checkers_view(st)))
                        } else {
                            None
                        };
                        ScreenView::GameShell {
                            game,
                            mode,
                            status,
                            checkers,
                        }
                    } else {
                        self.checkers_state = None;
                        self.selected_path.clear();
                        ScreenView::Home
                    };
                    self.dirty = true;
                }
                ServerMessage::CheckersSnapshot { state } => {
                    let turn_changed = self
                        .checkers_state
                        .as_ref()
                        .is_none_or(|prev| prev.turn_number != state.turn_number);
                    if turn_changed {
                        self.selected_path.clear();
                    }
                    self.checkers_state = Some(state);
                    self.sync_checkers_view();
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
    use boomer_protocol::{BotLevelDto, CheckersColorDto};

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
        match &app.view_model().screen {
            ScreenView::GameShell {
                game,
                checkers: Some(view),
                ..
            } => {
                assert_eq!(*game, GameKindDto::Checkers);
                assert_eq!(view.state.turn, CheckersColorDto::Black);
                assert_eq!(view.state.legal_moves.len(), 7);
            }
            other => panic!("unexpected screen: {other:?}"),
        }

        let changed = app.handle_input(UiInput::ReturnHome);
        assert!(changed);
        assert_eq!(app.view_model().screen, ScreenView::Home);
    }

    #[test]
    fn checkers_click_move_triggers_time_sliced_bot_reply() {
        let mut app = ClientApp::new(LocalSessionAdapter::with_seed(99));
        let _ = app.handle_input(UiInput::StartLocalGame {
            game: GameKindDto::Checkers,
            mode: LocalModeDto::VsBot {
                level: BotLevelDto::Easy,
            },
        });

        // Human plays 11-15 via two clicks.
        assert!(app.handle_input(UiInput::CheckersSelectSquare { square: 11 }));
        assert!(app.handle_input(UiInput::CheckersSelectSquare { square: 15 }));

        // Bot should now be thinking.
        if let ScreenView::GameShell {
            checkers: Some(view),
            ..
        } = &app.view_model().screen
        {
            assert!(view.state.bot_thinking);
            assert_eq!(view.state.moves, vec!["11-15"]);
        } else {
            panic!("expected checkers shell");
        }

        // Advance ~600ms across 16ms frames so time-sliced bot completes & moves.
        for _ in 0..40 {
            let _ = app.tick(16);
        }

        if let ScreenView::GameShell {
            checkers: Some(view),
            ..
        } = &app.view_model().screen
        {
            assert!(!view.state.bot_thinking);
            assert_eq!(view.state.moves.len(), 2);
            assert_eq!(view.state.turn, CheckersColorDto::Black);
        } else {
            panic!("expected checkers shell");
        }
    }

    #[test]
    fn checkers_rematch_alternates_starting_colors() {
        let mut app = ClientApp::new(LocalSessionAdapter::with_seed(42));
        let _ = app.handle_input(UiInput::StartLocalGame {
            game: GameKindDto::Checkers,
            mode: LocalModeDto::Hotseat,
        });
        let _ = app.handle_input(UiInput::Rematch);
        if let ScreenView::GameShell {
            checkers: Some(view),
            ..
        } = &app.view_model().screen
        {
            assert_eq!(
                view.state.seat_colors,
                [CheckersColorDto::White, CheckersColorDto::Black]
            );
        } else {
            panic!("expected checkers shell");
        }
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
                checkers: None,
            }
        );
        assert_eq!(app.pool_frame(0.0).len(), POOL_FRAME_LEN);
    }
}
