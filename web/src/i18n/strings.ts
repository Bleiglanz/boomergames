export type Lang = "en" | "de";

export interface Strings {
  readonly subtitle: string;
  readonly offlineBadge: string;
  readonly highContrast: string;
  readonly returnHome: string;
  readonly checkersTitle: string;
  readonly checkersDesc: string;
  readonly poolTitle: string;
  readonly poolDesc: string;
  readonly hotseat: string;
  readonly botEasy: string;
  readonly botMedium: string;
  readonly botHard: string;
  readonly freePractice: string;
  readonly statusLabel: string;
  readonly modeLabel: string;
  readonly shellCheckersReady: string;
  readonly shellPoolReady: string;
  readonly footerNote: string;
}

const EN: Strings = {
  subtitle: "Classic Offline Lounge",
  offlineBadge: "Offline WASM",
  highContrast: "High Contrast",
  returnHome: "← Back to Games",
  checkersTitle: "Checkers (English Draughts)",
  checkersDesc: "8×8 classic board, mandatory captures, multi-jumps, and 3 computer skill levels.",
  poolTitle: "8-Ball Pool",
  poolDesc: "Classic 2D top-down table, deterministic physics, English spin, and 3 computer skill levels.",
  hotseat: "Hotseat (2P)",
  botEasy: "vs. Bot (Easy)",
  botMedium: "vs. Bot (Medium)",
  botHard: "vs. Bot (Hard)",
  freePractice: "Free Practice",
  statusLabel: "Status",
  modeLabel: "Mode",
  shellCheckersReady: "Checkers orthographic Three.js stage connected to Rust/WASM (Milestone 1 engine next).",
  shellPoolReady: "8-Ball Pool orthographic Three.js stage driven by Rust/WASM Float64Array rack (Milestone 2 physics next).",
  footerNote: "100% free · No cookies · Runs locally in Rust/WebAssembly",
};

const DE: Strings = {
  subtitle: "Klassische Offline-Lounge",
  offlineBadge: "Offline WASM",
  highContrast: "Hoher Kontrast",
  returnHome: "← Zurück zur Auswahl",
  checkersTitle: "Dame (English Draughts)",
  checkersDesc: "Klassisches 8×8-Brett, Schlagzwang, Mehrfachsprünge und 3 Computer-Stufen.",
  poolTitle: "8-Ball Billard",
  poolDesc: "Klassischer 2D-Tisch von oben, deterministische Physik, Effet und 3 Computer-Stufen.",
  hotseat: "Lokal (2 Spieler)",
  botEasy: "gegen Bot (Leicht)",
  botMedium: "gegen Bot (Mittel)",
  botHard: "gegen Bot (Schwer)",
  freePractice: "Freies Training",
  statusLabel: "Status",
  modeLabel: "Modus",
  shellCheckersReady: "Dame-Bühne (Three.js 2D) mit Rust/WASM verbunden (Meilenstein 1 folgt).",
  shellPoolReady: "8-Ball-Bühne (Three.js 2D) über Rust/WASM Float64Array verbunden (Meilenstein 2 folgt).",
  footerNote: "100 % kostenlos · Keine Cookies · Läuft lokal in Rust/WebAssembly",
};

export function getStrings(lang: string): Strings {
  return lang === "de" ? DE : EN;
}
