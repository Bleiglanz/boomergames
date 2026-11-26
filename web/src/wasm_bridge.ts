import initWasm, { BoomerClient, BoomerRemoteClient } from "./wasm/boomer_client.js";

export type GameKindDto = "checkers" | "pool" | "chess";
export type BotLevelDto = "easy" | "medium" | "hard";
export type SeatDto = "1" | "2";
export type RemoteRole = "offline" | "seat1" | "seat2" | "spectator";

export type LocalModeDto =
  | { readonly kind: "hotseat" }
  | { readonly kind: "vs_bot"; readonly level: BotLevelDto }
  | { readonly kind: "free_practice" };

export type CheckersClockModeDto = "move" | "game" | "unlimited";
export type CheckersColorDto = "black" | "white";

export interface CheckersSettingsDto {
  readonly clockMode: CheckersClockModeDto;
  readonly moveTimerSecs: number;
  readonly gameMinutes: number;
}

export interface CheckersMoveDto {
  readonly path: readonly number[];
  readonly captured: readonly number[];
  readonly crowned: boolean;
  readonly notation: string;
}

export type CheckersOutcomeDto =
  | {
      readonly kind: "win";
      readonly winner: CheckersColorDto;
      readonly winner_seat: SeatDto;
      readonly reason: "no_moves" | "resign" | "timeout";
    }
  | {
      readonly kind: "draw";
      readonly reason: "agreement" | "threefold_repetition" | "forty_move_rule";
    };

export interface CheckersStateDto {
  readonly board: string;
  readonly turn: CheckersColorDto;
  readonly activeSeat: SeatDto;
  readonly seatColors: readonly [CheckersColorDto, CheckersColorDto];
  readonly turnNumber: number;
  readonly moves: readonly string[];
  readonly lastMove: readonly number[] | null;
  readonly mandatoryCapture: boolean;
  readonly legalMoves: readonly CheckersMoveDto[];
  readonly capturedByBlack: number;
  readonly capturedByWhite: number;
  readonly settings: CheckersSettingsDto;
  readonly clocksMs: readonly [number, number];
  readonly turnMsLeft: number;
  readonly drawOfferedBy: SeatDto | null;
  readonly botThinking: boolean;
  readonly outcome: CheckersOutcomeDto | null;
}

export interface CheckersView {
  readonly state: CheckersStateDto;
  readonly selectedPath: readonly number[];
  readonly selectableSquares: readonly number[];
  readonly highlightedTargets: readonly number[];
  readonly flipped: boolean;
  readonly showSquareNumbers: boolean;
}

export type ChessColorDto = "white" | "black";

export interface ChessSettingsDto {
  readonly clockMode: CheckersClockModeDto;
  readonly moveTimerSecs: number;
  readonly gameMinutes: number;
}

export interface ChessMoveDto {
  readonly from: number;
  readonly to: number;
  readonly uci: string;
  readonly promotion?: string;
  readonly isCapture: boolean;
}

export type ChessOutcomeDto =
  | {
      readonly kind: "win";
      readonly winner: ChessColorDto;
      readonly winner_seat: SeatDto;
      readonly reason: "checkmate" | "resign" | "timeout";
    }
  | {
      readonly kind: "draw";
      readonly reason:
        | "stalemate"
        | "agreement"
        | "threefold_repetition"
        | "fifty_move_rule"
        | "insufficient_material";
    };

export interface ChessStateDto {
  readonly board: string;
  readonly fen: string;
  readonly turn: ChessColorDto;
  readonly activeSeat: SeatDto;
  readonly seatColors: readonly [ChessColorDto, ChessColorDto];
  readonly turnNumber: number;
  readonly moves: readonly string[];
  readonly lastMove: readonly [number, number] | null;
  readonly inCheck: boolean;
  readonly checkSquare: number | null;
  readonly legalMoves: readonly ChessMoveDto[];
  readonly capturedByWhite: readonly string[];
  readonly capturedByBlack: readonly string[];
  readonly settings: ChessSettingsDto;
  readonly clocksMs: readonly [number, number];
  readonly turnMsLeft: number;
  readonly drawOfferedBy: SeatDto | null;
  readonly botThinking: boolean;
  readonly outcome: ChessOutcomeDto | null;
}

export interface ChessPendingPromotion {
  readonly from: number;
  readonly to: number;
}

export interface ChessView {
  readonly state: ChessStateDto;
  readonly selectedSquare: number | null;
  readonly selectableSquares: readonly number[];
  readonly highlightedTargets: readonly number[];
  readonly pendingPromotion: ChessPendingPromotion | null;
  readonly flipped: boolean;
  readonly showCoords: boolean;
}

export type BallGroupDto = "solids" | "stripes";
export type AimAidLevelDto = "off" | "basic" | "extended";
export type BallInHandRuleDto = "anywhere" | "behind_head_string";
export type EightOnBreakRuleDto = "re_rack" | "win";
export type PoolFoulDto = "scratch" | "wrong_first_ball" | "no_hit" | "no_rail_after_contact";
export type PoolWinReasonDto =
  | "eight_ball_pocketed"
  | "opponent_early_eight"
  | "opponent_foul_on_eight"
  | "resign";

export interface PoolSettingsDto {
  readonly aimAid: AimAidLevelDto;
  readonly callEight: boolean;
  readonly ballInHand: BallInHandRuleDto;
  readonly eightOnBreak: EightOnBreakRuleDto;
  readonly shotTimerSecs: number;
}

export interface PoolOutcomeDto {
  readonly winnerSeat: SeatDto;
  readonly reason: PoolWinReasonDto;
}

export interface PoolBallDto {
  readonly id: number;
  readonly x: number;
  readonly y: number;
  readonly pocketed: boolean;
}

export interface PoolStateDto {
  readonly balls: readonly PoolBallDto[];
  readonly activeSeat: SeatDto;
  readonly groups: readonly [BallGroupDto | null, BallGroupDto | null];
  readonly pocketedBySeat: readonly [readonly number[], readonly number[]];
  readonly isBreakShot: boolean;
  readonly ballInHand: boolean;
  readonly ballInHandBehindLine: boolean;
  readonly isOnEightBall: boolean;
  readonly freePractice: boolean;
  readonly turnNumber: number;
  readonly settings: PoolSettingsDto;
  readonly turnMsLeft: number;
  readonly lastFouls: readonly PoolFoulDto[];
  readonly lastPotted: readonly number[];
  readonly reracked: boolean;
  readonly botThinking: boolean;
  readonly outcome: PoolOutcomeDto | null;
}

export interface PoolAimPreviewView {
  readonly cuePos: readonly [number, number];
  readonly ghostBall: readonly [number, number];
  readonly targetBall: number | null;
  readonly objectLine: readonly [readonly [number, number], readonly [number, number]] | null;
  readonly cueDeflectionLine:
    | readonly [readonly [number, number], readonly [number, number]]
    | null;
}

export interface PoolCuePreviewView {
  readonly pos: readonly [number, number];
  readonly valid: boolean;
}

export interface PoolView {
  readonly state: PoolStateDto;
  readonly animating: boolean;
  readonly aimAngle: number;
  readonly power: number;
  readonly spinX: number;
  readonly spinY: number;
  readonly calledPocket: number | null;
  readonly aimPreview: PoolAimPreviewView | null;
  readonly cuePreview: PoolCuePreviewView | null;
}

export type ScreenView =
  | { readonly kind: "home" }
  | {
      readonly kind: "game_shell";
      readonly game: GameKindDto;
      readonly mode: LocalModeDto;
      readonly status: string;
      readonly checkers: CheckersView | null;
      readonly pool: PoolView | null;
      readonly chess: ChessView | null;
    };

export interface ViewModel {
  readonly version: string;
  readonly sessionKind: "local" | "remote";
  readonly remoteRole: RemoteRole;
  readonly screen: ScreenView;
  readonly highContrast: boolean;
  readonly lang: string;
  readonly lastError: string | null;
}

export type UiInput =
  | { readonly kind: "start_local_game"; readonly game: GameKindDto; readonly mode: LocalModeDto }
  | { readonly kind: "return_home" }
  | { readonly kind: "toggle_high_contrast" }
  | { readonly kind: "set_language"; readonly lang: "en" | "de" }
  | { readonly kind: "checkers_select_square"; readonly square: number }
  | { readonly kind: "checkers_drag_move"; readonly from: number; readonly to: number }
  | { readonly kind: "checkers_clear_selection" }
  | { readonly kind: "checkers_flip_board" }
  | { readonly kind: "checkers_toggle_square_numbers" }
  | { readonly kind: "checkers_set_clock"; readonly settings: CheckersSettingsDto }
  | { readonly kind: "chess_select_square"; readonly square: number }
  | { readonly kind: "chess_drag_move"; readonly from: number; readonly to: number }
  | { readonly kind: "chess_promote"; readonly piece: string }
  | { readonly kind: "chess_clear_selection" }
  | { readonly kind: "chess_flip_board" }
  | { readonly kind: "chess_toggle_coords" }
  | { readonly kind: "chess_set_clock"; readonly settings: ChessSettingsDto }
  | { readonly kind: "chess_apply_bot_move"; readonly uci: string }
  | { readonly kind: "pool_set_aim"; readonly angle: number }
  | { readonly kind: "pool_set_power"; readonly power: number }
  | { readonly kind: "pool_set_spin"; readonly spin_x: number; readonly spin_y: number }
  | { readonly kind: "pool_call_pocket"; readonly pocket: number }
  | { readonly kind: "pool_preview_cue"; readonly x: number; readonly y: number }
  | { readonly kind: "pool_place_cue"; readonly x: number; readonly y: number }
  | { readonly kind: "pool_cancel_cue_preview" }
  | { readonly kind: "pool_shoot" }
  | { readonly kind: "pool_set_settings"; readonly settings: PoolSettingsDto }
  | { readonly kind: "pool_rerack" }
  | { readonly kind: "resign" }
  | { readonly kind: "offer_draw" }
  | { readonly kind: "answer_draw"; readonly accept: boolean }
  | { readonly kind: "rematch" };

export type SoundCue =
  | { readonly kind: "cue_hit"; readonly intensity: number }
  | { readonly kind: "ball_click"; readonly intensity: number }
  | { readonly kind: "cushion_thud"; readonly intensity: number }
  | { readonly kind: "pocket_drop" }
  | { readonly kind: "checker_move" }
  | { readonly kind: "checker_capture" }
  | { readonly kind: "checker_king" }
  | { readonly kind: "clock_warn" }
  | { readonly kind: "game_over" };

export interface GameBridge {
  dispatch(input: UiInput): boolean;
  tick(dtMs: number): boolean;
  viewModel(): ViewModel;
  poolFrame(tMs: number): Float64Array;
  drainSoundCues(): readonly SoundCue[];
}

export class WasmBridge implements GameBridge {
  private readonly client: BoomerClient;

  private constructor(client: BoomerClient) {
    this.client = client;
  }

  public static async create(): Promise<WasmBridge> {
    await initWasm();
    return new WasmBridge(new BoomerClient());
  }

  public dispatch(input: UiInput): boolean {
    return this.client.dispatchInput(JSON.stringify(input));
  }

  public tick(dtMs: number): boolean {
    return this.client.tick(Math.max(0, Math.min(250, Math.round(dtMs))));
  }

  public viewModel(): ViewModel {
    return JSON.parse(this.client.viewModelJson()) as ViewModel;
  }

  public poolFrame(tMs: number): Float64Array {
    return this.client.poolFrame(tMs);
  }

  public drainSoundCues(): readonly SoundCue[] {
    const raw = this.client.drainSoundCuesJson();
    if (raw === "[]") {
      return [];
    }
    return JSON.parse(raw) as SoundCue[];
  }
}

export class RemoteWasmBridge implements GameBridge {
  private readonly client: BoomerRemoteClient;
  private disposed = false;

  public constructor(game: GameKindDto, role: Exclude<RemoteRole, "offline">) {
    this.client = new BoomerRemoteClient(game, role);
  }

  public pushServerMessage(rawJson: string): boolean {
    if (this.disposed) {
      return false;
    }
    return this.client.pushServerMessage(rawJson);
  }

  public drainClientMessages(): readonly string[] {
    if (this.disposed) {
      return [];
    }
    const raw = this.client.drainClientMessagesJson();
    if (raw === "[]") {
      return [];
    }
    const msgs = JSON.parse(raw) as unknown[];
    return msgs.map((m) => JSON.stringify(m));
  }

  public dispatch(input: UiInput): boolean {
    if (this.disposed) {
      return false;
    }
    return this.client.dispatchInput(JSON.stringify(input));
  }

  public tick(dtMs: number): boolean {
    if (this.disposed) {
      return false;
    }
    return this.client.tick(Math.max(0, Math.min(250, Math.round(dtMs))));
  }

  public viewModel(): ViewModel {
    return JSON.parse(this.client.viewModelJson()) as ViewModel;
  }

  public poolFrame(tMs: number): Float64Array {
    return this.client.poolFrame(tMs);
  }

  public drainSoundCues(): readonly SoundCue[] {
    if (this.disposed) {
      return [];
    }
    const raw = this.client.drainSoundCuesJson();
    if (raw === "[]") {
      return [];
    }
    return JSON.parse(raw) as SoundCue[];
  }

  public dispose(): void {
    if (!this.disposed) {
      this.disposed = true;
      this.client.free();
    }
  }
}
