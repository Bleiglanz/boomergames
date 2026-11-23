import initWasm, { BoomerClient } from "./wasm/boomer_client.js";

export type GameKindDto = "checkers" | "pool";
export type BotLevelDto = "easy" | "medium" | "hard";
export type SeatDto = "1" | "2";

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

export type ScreenView =
  | { readonly kind: "home" }
  | {
      readonly kind: "game_shell";
      readonly game: GameKindDto;
      readonly mode: LocalModeDto;
      readonly status: string;
      readonly checkers: CheckersView | null;
    };

export interface ViewModel {
  readonly version: string;
  readonly sessionKind: "local" | "remote";
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
  | { readonly kind: "resign" }
  | { readonly kind: "offer_draw" }
  | { readonly kind: "answer_draw"; readonly accept: boolean }
  | { readonly kind: "rematch" };

export class WasmBridge {
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
}
