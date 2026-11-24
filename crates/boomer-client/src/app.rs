//! Client application state machine projecting `ViewModel` snapshots for the DOM/Three.js shell.

use boomer_domain::math::{Vec2, clamp};
use boomer_domain::pool::constants::{
    BALL_DIAMETER, BALL_RADIUS, HEAD_STRING_X, TABLE_HEIGHT, TABLE_WIDTH,
};
use boomer_domain::pool::{compute_aim_preview, simulate_shot};
use boomer_protocol::{
    CheckersSettingsDto, CheckersStateDto, ClientMessage, ErrorCode, GameKindDto, LocalModeDto,
    PROTOCOL_VERSION, PoolSettingsDto, PoolShotDto, PoolStateDto, ServerMessage, SessionKindDto,
};
use serde::{Deserialize, Serialize};

use crate::animation::{POOL_FRAME_LEN, PoolAnimationBuffer, SoundCue};
use crate::mapping::{aim_aid_from_dto, pool_balls_from_dto, pool_shot_from_dto};
use crate::ports::{GameTransport, TransportError};

/// Discrete UI inputs sent from the thin TypeScript layer into WASM.
#[derive(Debug, Clone, PartialEq, Deserialize)]
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
    /// Update live Pool cue aim angle (radians).
    PoolSetAim {
        /// Aim angle in radians.
        angle: f64,
    },
    /// Update live Pool shot power (`[0.02, 1.0]`).
    PoolSetPower {
        /// Normalized shot power.
        power: f64,
    },
    /// Update live Pool English spin (`[-1.0, 1.0]`).
    PoolSetSpin {
        /// Side English (`-1` left, `+1` right).
        spin_x: f64,
        /// Top/Back spin (`+1` follow, `-1` draw).
        spin_y: f64,
    },
    /// Select a called pocket (`0..6`) for 8-ball call-pocket mode.
    PoolCallPocket {
        /// Pocket index (`0..6`).
        pocket: u8,
    },
    /// Live preview of cue-ball drag position during ball-in-hand.
    PoolPreviewCue {
        /// Candidate X in metres.
        x: f64,
        /// Candidate Y in metres.
        y: f64,
    },
    /// Commit cue-ball placement during ball-in-hand.
    PoolPlaceCue {
        /// Target X in metres.
        x: f64,
        /// Target Y in metres.
        y: f64,
    },
    /// Clear temporary ball-in-hand drag preview.
    PoolCancelCuePreview,
    /// Execute the current aimed Pool shot.
    PoolShoot,
    /// Update Pool table settings (aim aid, call-8, ball-in-hand, timer).
    PoolSetSettings {
        /// Updated Pool settings.
        settings: PoolSettingsDto,
    },
    /// Re-rack table in Free Practice mode.
    PoolRerack,
    /// Resign the active match.
    Resign,
    /// Offer a draw (Checkers).
    OfferDraw,
    /// Answer a pending draw offer (Checkers).
    AnswerDraw {
        /// `true` to accept, `false` to decline.
        accept: bool,
    },
    /// Request a rematch (alternates colors/breaker).
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

/// Aiming-aid geometry projected from `boomer-domain::pool::aim`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolAimPreviewView {
    /// Cue ball center `[x, y]`.
    pub cue_pos: [f64; 2],
    /// Ghost-ball contact center `[x, y]`.
    pub ghost_ball: [f64; 2],
    /// First contacted object ball ID, if any.
    pub target_ball: Option<u8>,
    /// Predicted object-ball travel vector (`Extended` aid).
    pub object_line: Option<[[f64; 2]; 2]>,
    /// Predicted cue-ball tangent line (`Extended` aid).
    pub cue_deflection_line: Option<[[f64; 2]; 2]>,
}

/// Live cue-ball drag legality preview during ball-in-hand.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolCuePreviewView {
    /// Preview `[x, y]` coordinate in metres.
    pub pos: [f64; 2],
    /// Whether this spot is legally valid (green) or overlapping/out-of-bounds (red).
    pub valid: bool,
}

/// Interactive 8-Ball Pool UI projection derived strictly in Rust.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolView {
    /// Authoritative Pool state (held at pre-shot state while animation plays, updated on rest).
    pub state: PoolStateDto,
    /// `true` while balls are in motion (locks all aiming/shooting controls).
    pub animating: bool,
    /// Current aim angle in radians.
    pub aim_angle: f64,
    /// Current shot power in `[0.02, 1.0]`.
    pub power: f64,
    /// Side English in `[-1.0, 1.0]`.
    pub spin_x: f64,
    /// Top/Back spin in `[-1.0, 1.0]`.
    pub spin_y: f64,
    /// Called pocket (`0..6`) when shooting the 8-ball with call-pocket enabled.
    pub called_pocket: Option<u8>,
    /// Aiming-aid geometry computed in Rust.
    pub aim_preview: Option<PoolAimPreviewView>,
    /// Active ball-in-hand drag preview, if dragging the cue ball.
    pub cue_preview: Option<PoolCuePreviewView>,
}

/// Active screen projected to the UI shell.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
        /// 8-Ball Pool interactive state when `game == Pool`.
        pool: Option<Box<PoolView>>,
    },
}

/// Complete immutable UI projection serialized to JS only when state changes.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
    pool_state: Option<PoolStateDto>,
    pending_pool_state: Option<PoolStateDto>,
    pool_aim_angle: f64,
    pool_power: f64,
    pool_spin_x: f64,
    pool_spin_y: f64,
    pool_called_pocket: Option<u8>,
    pool_cue_preview: Option<PoolCuePreviewView>,
    sound_cues: Vec<SoundCue>,
    last_warn_second: Option<u32>,
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
            pool_state: None,
            pending_pool_state: None,
            pool_aim_angle: 0.0,
            pool_power: 0.65,
            pool_spin_x: 0.0,
            pool_spin_y: 0.0,
            pool_called_pocket: Some(0),
            pool_cue_preview: None,
            sound_cues: Vec::new(),
            last_warn_second: None,
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
                self.reset_transient_ui();
                self.send_checked(ClientMessage::LocalStart { game, mode });
            }
            UiInput::ReturnHome => {
                self.reset_transient_ui();
                self.checkers_state = None;
                self.pool_state = None;
                self.pending_pool_state = None;
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
            UiInput::CheckersSelectSquare { square } => self.handle_checkers_select(square),
            UiInput::CheckersDragMove { from, to } => self.handle_checkers_drag(from, to),
            UiInput::CheckersClearSelection => {
                if !self.selected_path.is_empty() {
                    self.selected_path.clear();
                    self.sync_game_views();
                }
            }
            UiInput::CheckersFlipBoard => {
                self.checkers_flipped = !self.checkers_flipped;
                self.sync_game_views();
            }
            UiInput::CheckersToggleSquareNumbers => {
                self.show_square_numbers = !self.show_square_numbers;
                self.sync_game_views();
            }
            UiInput::CheckersSetClock { settings } => {
                self.selected_path.clear();
                self.send_checked(ClientMessage::CheckersSettings { settings });
            }
            UiInput::PoolSetAim { .. }
            | UiInput::PoolSetPower { .. }
            | UiInput::PoolSetSpin { .. }
            | UiInput::PoolCallPocket { .. }
            | UiInput::PoolPreviewCue { .. }
            | UiInput::PoolPlaceCue { .. }
            | UiInput::PoolCancelCuePreview
            | UiInput::PoolShoot
            | UiInput::PoolSetSettings { .. }
            | UiInput::PoolRerack => self.handle_pool_ui_input(&input),
            UiInput::Resign => {
                self.reset_transient_ui();
                self.send_checked(ClientMessage::Resign {});
            }
            UiInput::OfferDraw => self.send_checked(ClientMessage::DrawOffer {}),
            UiInput::AnswerDraw { accept } => {
                self.send_checked(ClientMessage::DrawAnswer { accept });
            }
            UiInput::Rematch => {
                self.reset_transient_ui();
                self.send_checked(ClientMessage::Rematch {});
            }
        }
        self.drain_transport();
        self.take_dirty()
    }

    /// Advances transport timers, time-sliced AI, and Pool shot animation by `dt_ms`.
    /// Returns `true` if `ViewModel` changed.
    pub fn tick(&mut self, dt_ms: u32) -> bool {
        if self.pool_anim.advance(dt_ms, &mut self.sound_cues) {
            if let Some(next_state) = self.pending_pool_state.take() {
                if next_state.outcome.is_some() {
                    self.sound_cues.push(SoundCue::GameOver);
                }
                let snap = pool_balls_from_dto(&next_state.balls);
                self.pool_anim.load_static(&snap);
                self.pool_state = Some(next_state);
                self.last_warn_second = None;
            }
            self.sync_game_views();
        }
        self.transport.tick(dt_ms);
        self.drain_transport();
        self.take_dirty()
    }

    /// Drains all discrete [`SoundCue`]s emitted since the last call.
    pub fn drain_sound_cues(&mut self) -> Vec<SoundCue> {
        core::mem::take(&mut self.sound_cues)
    }

    /// Returns a reference to the current [`ViewModel`].
    #[must_use]
    pub const fn view_model(&self) -> &ViewModel {
        &self.view
    }

    /// Samples the 60 fps Pool ball frame buffer at `t_ms`.
    #[must_use]
    pub const fn pool_frame(&self, t_ms: f64) -> &[f64; POOL_FRAME_LEN] {
        self.pool_anim.sample(t_ms)
    }

    fn reset_transient_ui(&mut self) {
        self.selected_path.clear();
        self.pool_cue_preview = None;
        self.pool_spin_x = 0.0;
        self.pool_spin_y = 0.0;
        self.pool_aim_angle = 0.0;
        self.pool_power = 0.65;
    }

    fn send_checked(&mut self, msg: ClientMessage) {
        if let Err(err) = self.transport.send(msg) {
            self.record_transport_error(&err);
        }
    }

    fn handle_pool_ui_input(&mut self, input: &UiInput) {
        match *input {
            UiInput::PoolSetAim { angle } => {
                if angle.is_finite() {
                    self.pool_aim_angle = angle;
                    self.sync_game_views();
                }
            }
            UiInput::PoolSetPower { power } => {
                if power.is_finite() {
                    self.pool_power = clamp(power, 0.02, 1.0);
                    self.sync_game_views();
                }
            }
            UiInput::PoolSetSpin { spin_x, spin_y } => {
                if spin_x.is_finite() && spin_y.is_finite() {
                    let v = Vec2::new(spin_x, spin_y);
                    let len = v.len();
                    let clamped = if len > 1.0 { v.scale(1.0 / len) } else { v };
                    self.pool_spin_x = clamped.x;
                    self.pool_spin_y = clamped.y;
                    self.sync_game_views();
                }
            }
            UiInput::PoolCallPocket { pocket } => {
                if pocket < 6 {
                    self.pool_called_pocket = Some(pocket);
                    self.sync_game_views();
                }
            }
            UiInput::PoolPreviewCue { x, y } => self.handle_pool_preview_cue(x, y),
            UiInput::PoolPlaceCue { x, y } => {
                self.pool_cue_preview = None;
                self.send_checked(ClientMessage::PoolPlaceCue { x, y });
            }
            UiInput::PoolCancelCuePreview => {
                if self.pool_cue_preview.is_some() {
                    self.pool_cue_preview = None;
                    self.sync_game_views();
                }
            }
            UiInput::PoolShoot => self.handle_pool_shoot(),
            UiInput::PoolSetSettings { settings } => {
                self.send_checked(ClientMessage::PoolSettings { settings });
            }
            UiInput::PoolRerack => {
                self.pool_cue_preview = None;
                self.send_checked(ClientMessage::PoolRerack {});
            }
            _ => {}
        }
    }

    fn handle_pool_preview_cue(&mut self, x: f64, y: f64) {
        let Some(state) = &self.pool_state else {
            return;
        };
        if !state.ball_in_hand || self.pool_anim.is_playing() || state.bot_thinking {
            return;
        }
        let valid = is_preview_cue_valid(state, Vec2::new(x, y));
        self.pool_cue_preview = Some(PoolCuePreviewView { pos: [x, y], valid });
        self.sync_game_views();
    }

    fn handle_pool_shoot(&mut self) {
        let Some(state) = &self.pool_state else {
            return;
        };
        if self.pool_anim.is_playing() || state.outcome.is_some() || state.bot_thinking {
            return;
        }
        let cue_pos = if state.ball_in_hand {
            state.balls.iter().find(|b| b.id == 0).map(|b| [b.x, b.y])
        } else {
            None
        };
        let shot = PoolShotDto {
            turn: state.turn_number,
            angle: self.pool_aim_angle,
            power: self.pool_power,
            spin_x: self.pool_spin_x,
            spin_y: self.pool_spin_y,
            cue_pos,
            called_pocket: self.pool_called_pocket,
        };
        self.pool_cue_preview = None;
        self.send_checked(ClientMessage::PoolShoot { shot });
    }

    fn handle_checkers_select(&mut self, square: u8) {
        let Some(state) = self.checkers_state.clone() else {
            return;
        };
        if state.outcome.is_some() || state.bot_thinking {
            return;
        }

        if !self.selected_path.is_empty() {
            if self.try_advance_or_submit(&state, square) {
                return;
            }
            if self.selected_path.len() == 1 && self.selected_path.first() == Some(&square) {
                self.selected_path.clear();
                self.sync_game_views();
                return;
            }
        }

        let can_start = state
            .legal_moves
            .iter()
            .any(|mv| mv.path.first() == Some(&square));
        if can_start {
            self.selected_path = vec![square];
            self.sync_game_views();
        } else if !self.selected_path.is_empty() {
            self.selected_path.clear();
            self.sync_game_views();
        }
    }

    fn handle_checkers_drag(&mut self, from: u8, to: u8) {
        let Some(state) = self.checkers_state.clone() else {
            return;
        };
        if state.outcome.is_some() || state.bot_thinking {
            return;
        }
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
            self.sync_game_views();
        }
    }

    fn try_advance_or_submit(&mut self, state: &CheckersStateDto, target: u8) -> bool {
        let prefix = &self.selected_path;
        let matching: Vec<&boomer_protocol::CheckersMoveDto> = state
            .legal_moves
            .iter()
            .filter(|mv| mv.path.starts_with(prefix))
            .collect();

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
            self.sync_game_views();
            return true;
        }

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

            let next_idx = self.selected_path.len();
            for mv in &matching {
                if let Some(&next_sq) = mv.path.get(next_idx)
                    && !highlighted.contains(&next_sq)
                {
                    highlighted.push(next_sq);
                }
            }

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

    fn build_pool_view(&self, state: &PoolStateDto) -> PoolView {
        let animating = self.pool_anim.is_playing();
        let aim_preview = if !animating && state.outcome.is_none() && !state.bot_thinking {
            let domain_balls = pool_balls_from_dto(&state.balls);
            domain_balls.first().filter(|cue| !cue.pocketed).map(|cue| {
                let preview = compute_aim_preview(
                    &domain_balls,
                    cue.pos,
                    self.pool_aim_angle,
                    aim_aid_from_dto(state.settings.aim_aid),
                );
                PoolAimPreviewView {
                    cue_pos: [preview.cue_pos.x, preview.cue_pos.y],
                    ghost_ball: [preview.ghost_ball.x, preview.ghost_ball.y],
                    target_ball: preview.target_ball,
                    object_line: preview.object_line.map(|(a, b)| [[a.x, a.y], [b.x, b.y]]),
                    cue_deflection_line: preview
                        .cue_deflection_line
                        .map(|(a, b)| [[a.x, a.y], [b.x, b.y]]),
                }
            })
        } else {
            None
        };

        PoolView {
            state: state.clone(),
            animating,
            aim_angle: self.pool_aim_angle,
            power: self.pool_power,
            spin_x: self.pool_spin_x,
            spin_y: self.pool_spin_y,
            called_pocket: self.pool_called_pocket,
            aim_preview,
            cue_preview: self.pool_cue_preview,
        }
    }

    fn sync_game_views(&mut self) {
        let checkers_next = self
            .checkers_state
            .as_ref()
            .map(|st| Box::new(self.build_checkers_view(st)));
        let pool_next = self
            .pool_state
            .as_ref()
            .map(|st| Box::new(self.build_pool_view(st)));
        if let ScreenView::GameShell { checkers, pool, .. } = &mut self.view.screen {
            *checkers = checkers_next;
            *pool = pool_next;
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
                        let pool = if game == GameKindDto::Pool {
                            self.pool_state
                                .as_ref()
                                .map(|st| Box::new(self.build_pool_view(st)))
                        } else {
                            None
                        };
                        ScreenView::GameShell {
                            game,
                            mode,
                            status,
                            checkers,
                            pool,
                        }
                    } else {
                        self.checkers_state = None;
                        self.pool_state = None;
                        self.pending_pool_state = None;
                        self.selected_path.clear();
                        ScreenView::Home
                    };
                    self.dirty = true;
                }
                ServerMessage::CheckersSnapshot { state } => {
                    self.handle_checkers_snapshot(state);
                }
                ServerMessage::PoolSnapshot { state } => {
                    if self.pool_anim.is_playing() {
                        self.pending_pool_state = Some(state);
                    } else {
                        self.check_pool_clock_warning(&state);
                        let prev_outcome = self.pool_state.as_ref().and_then(|p| p.outcome);
                        if prev_outcome.is_none() && state.outcome.is_some() {
                            self.sound_cues.push(SoundCue::GameOver);
                        }
                        let balls = pool_balls_from_dto(&state.balls);
                        self.pool_anim.load_static(&balls);
                        self.pool_state = Some(state);
                        self.pending_pool_state = None;
                        self.sync_game_views();
                    }
                }
                ServerMessage::PoolShot {
                    pre_balls,
                    shot,
                    state,
                } => {
                    let pre = pool_balls_from_dto(&pre_balls);
                    let snap = pool_balls_from_dto(&state.balls);
                    let sim = simulate_shot(&pre, pool_shot_from_dto(shot), true);
                    self.pool_anim.start_replay(&sim, &snap, shot.power);
                    self.last_warn_second = None;
                    if self.pool_anim.is_playing() {
                        self.pending_pool_state = Some(state);
                    } else {
                        self.pool_state = Some(state);
                        self.pending_pool_state = None;
                    }
                    self.pool_spin_x = 0.0;
                    self.pool_spin_y = 0.0;
                    self.sync_game_views();
                }
                ServerMessage::Error { code, .. } => {
                    self.view.last_error = Some(code);
                    self.dirty = true;
                }
            }
        }
    }

    fn handle_checkers_snapshot(&mut self, state: CheckersStateDto) {
        let prev_moves = self.checkers_state.as_ref().map_or(0, |p| p.moves.len());
        let prev_outcome = self.checkers_state.as_ref().and_then(|p| p.outcome);
        let turn_changed = self
            .checkers_state
            .as_ref()
            .is_none_or(|prev| prev.turn_number != state.turn_number);
        if turn_changed {
            self.selected_path.clear();
            self.last_warn_second = None;
        }
        if state.moves.len() > prev_moves
            && let Some(last_notation) = state.moves.last()
        {
            let prev_kings = self.checkers_state.as_ref().map_or(0, |p| {
                p.board.bytes().filter(|&b| b == b'B' || b == b'W').count()
            });
            let next_kings = state
                .board
                .bytes()
                .filter(|&b| b == b'B' || b == b'W')
                .count();
            if next_kings > prev_kings {
                self.sound_cues.push(SoundCue::CheckerKing);
            } else if last_notation.contains('x') {
                self.sound_cues.push(SoundCue::CheckerCapture);
            } else {
                self.sound_cues.push(SoundCue::CheckerMove);
            }
        }
        if prev_outcome.is_none() && state.outcome.is_some() {
            self.sound_cues.push(SoundCue::GameOver);
        }
        self.checkers_state = Some(state);
        self.sync_game_views();
    }

    fn check_pool_clock_warning(&mut self, state: &PoolStateDto) {
        if state.free_practice
            || state.outcome.is_some()
            || state.bot_thinking
            || state.settings.shot_timer_secs == 0
        {
            return;
        }
        let secs_left = state.turn_ms_left.div_ceil(1_000);
        if (1..=5).contains(&secs_left) && self.last_warn_second != Some(secs_left) {
            self.last_warn_second = Some(secs_left);
            self.sound_cues.push(SoundCue::ClockWarn);
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

fn is_preview_cue_valid(state: &PoolStateDto, pos: Vec2) -> bool {
    if !pos.x.is_finite() || !pos.y.is_finite() {
        return false;
    }
    let min_x = BALL_RADIUS + 0.002;
    let max_x = if state.ball_in_hand_behind_line {
        HEAD_STRING_X
    } else {
        TABLE_WIDTH - BALL_RADIUS - 0.002
    };
    let min_y = BALL_RADIUS + 0.002;
    let max_y = TABLE_HEIGHT - BALL_RADIUS - 0.002;
    if pos.x < min_x || pos.x > max_x || pos.y < min_y || pos.y > max_y {
        return false;
    }
    let min_sep_sq = (BALL_DIAMETER + 0.001) * (BALL_DIAMETER + 0.001);
    for b in &state.balls {
        if b.id != 0 && !b.pocketed && (Vec2::new(b.x, b.y) - pos).len_sq() < min_sep_sq {
            return false;
        }
    }
    true
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

        assert!(app.handle_input(UiInput::CheckersSelectSquare { square: 11 }));
        assert!(app.handle_input(UiInput::CheckersSelectSquare { square: 15 }));

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
    fn pool_break_shot_animates_and_settles_cleanly() {
        let mut app = ClientApp::new(LocalSessionAdapter::with_seed(77));
        let _ = app.handle_input(UiInput::StartLocalGame {
            game: GameKindDto::Pool,
            mode: LocalModeDto::FreePractice,
        });
        assert!(app.handle_input(UiInput::PoolSetAim { angle: 0.0 }));
        assert!(app.handle_input(UiInput::PoolSetPower { power: 0.9 }));
        assert!(app.handle_input(UiInput::PoolShoot));

        if let ScreenView::GameShell {
            pool: Some(view), ..
        } = &app.view_model().screen
        {
            assert!(view.animating);
        } else {
            panic!("expected pool shell");
        }

        // Advance simulation frames until settled.
        for _ in 0..1200 {
            let _ = app.tick(16);
        }

        if let ScreenView::GameShell {
            pool: Some(view), ..
        } = &app.view_model().screen
        {
            assert!(!view.animating);
            assert!(view.state.turn_number >= 2);
            assert!(view.aim_preview.is_some());
        } else {
            panic!("expected pool shell");
        }
        assert_eq!(app.pool_frame(0.0).len(), POOL_FRAME_LEN);
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
}
