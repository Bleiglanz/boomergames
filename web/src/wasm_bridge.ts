import initWasm, { BoomerClient } from "./wasm/boomer_client.js";

export type GameKindDto = "checkers" | "pool";
export type BotLevelDto = "easy" | "medium" | "hard";

export type LocalModeDto =
  | { readonly kind: "hotseat" }
  | { readonly kind: "vs_bot"; readonly level: BotLevelDto }
  | { readonly kind: "free_practice" };

export type ScreenView =
  | { readonly kind: "home" }
  | {
      readonly kind: "game_shell";
      readonly game: GameKindDto;
      readonly mode: LocalModeDto;
      readonly status: string;
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
  | { readonly kind: "set_language"; readonly lang: "en" | "de" };

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
