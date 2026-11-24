export type Lang = "en" | "de";

export interface Strings {
  readonly subtitle: string;
  readonly offlineBadge: string;
  readonly highContrast: string;
  readonly soundOn: string;
  readonly soundOff: string;
  readonly skipToContent: string;
  readonly landscapeHint: string;
  readonly poolKeysHint: string;
  readonly checkersKeysHint: string;
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
  readonly footerNote: string;
  readonly player1: string;
  readonly player2: string;
  readonly computer: string;
  readonly colorBlack: string;
  readonly colorWhite: string;
  readonly yourTurn: string;
  readonly botThinking: string;
  readonly mustCapture: string;
  readonly captured: string;
  readonly flipBoard: string;
  readonly showNumbers: string;
  readonly offerDraw: string;
  readonly resign: string;
  readonly rematch: string;
  readonly drawOfferedPrompt: string;
  readonly acceptDraw: string;
  readonly declineDraw: string;
  readonly clockPreset: string;
  readonly clockUnlimited: string;
  readonly clockMove30: string;
  readonly clockGame5: string;
  readonly clockGame10: string;
  readonly moveListTitle: string;
  readonly noMovesYet: string;
  readonly outcomeWinNoMoves: string;
  readonly outcomeWinResign: string;
  readonly outcomeWinTimeout: string;
  readonly outcomeDrawAgreement: string;
  readonly outcomeDrawRepetition: string;
  readonly outcomeDrawFortyMove: string;
  readonly drawDeclinedError: string;
  // Pool strings
  readonly poolOpenTable: string;
  readonly poolSolids: string;
  readonly poolStripes: string;
  readonly poolShootEight: string;
  readonly poolBreakHint: string;
  readonly poolBallInHand: string;
  readonly poolBallInHandBehind: string;
  readonly poolCallPocketHint: string;
  readonly poolShootBtn: string;
  readonly poolRerackBtn: string;
  readonly poolSpinLabel: string;
  readonly poolSpinCenter: string;
  readonly poolPowerLabel: string;
  readonly poolFineAimLabel: string;
  readonly poolAimAidLabel: string;
  readonly poolAimOff: string;
  readonly poolAimBasic: string;
  readonly poolAimExtended: string;
  readonly poolTimerLabel: string;
  readonly poolCallEightLabel: string;
  readonly poolFoulScratch: string;
  readonly poolFoulWrongBall: string;
  readonly poolFoulNoHit: string;
  readonly poolFoulNoRail: string;
  readonly poolWinEight: string;
  readonly poolWinEarlyEight: string;
  readonly poolWinFoulEight: string;
  readonly poolWinResign: string;
  readonly poolRerackedNotice: string;
}

const EN: Strings = {
  subtitle: "Classic Offline Lounge",
  offlineBadge: "Offline WASM",
  highContrast: "High Contrast",
  soundOn: "Sound: On",
  soundOff: "Sound: Off",
  skipToContent: "Skip to main content",
  landscapeHint: "Tip: Rotate your phone to landscape for a wider pool table.",
  poolKeysHint: "Keys: ←/→ aim (Shift = fine), ↑/↓ power, Space = shoot, Esc = cancel.",
  checkersKeysHint: "Keys: Arrows move cursor, Enter/Space select, Esc clear.",
  returnHome: "← Back to Games",
  checkersTitle: "Checkers (English Draughts)",
  checkersDesc: "8×8 classic board, mandatory captures, multi-jumps, and 3 computer skill levels.",
  poolTitle: "8-Ball Pool",
  poolDesc: "Classic 2D top-down table, deterministic Swept-CCD physics, English spin, and 3 computer skill levels.",
  hotseat: "Hotseat (2P)",
  botEasy: "vs. Bot (Easy)",
  botMedium: "vs. Bot (Medium)",
  botHard: "vs. Bot (Hard)",
  freePractice: "Free Practice",
  statusLabel: "Status",
  modeLabel: "Mode",
  footerNote: "100% free · No cookies · Runs locally in Rust/WebAssembly",
  player1: "Player 1",
  player2: "Player 2",
  computer: "Computer",
  colorBlack: "Black",
  colorWhite: "Red",
  yourTurn: "Turn",
  botThinking: "Computer is thinking…",
  mustCapture: "You must capture!",
  captured: "Captured",
  flipBoard: "Flip Board",
  showNumbers: "1–32 Numbers",
  offerDraw: "Offer Draw",
  resign: "Resign",
  rematch: "Rematch (Swap Turn)",
  drawOfferedPrompt: "Draw offered — accept?",
  acceptDraw: "Accept Draw",
  declineDraw: "Decline",
  clockPreset: "Clock",
  clockUnlimited: "No Timer",
  clockMove30: "30s / move",
  clockGame5: "5 min game",
  clockGame10: "10 min game",
  moveListTitle: "Moves",
  noMovesYet: "No moves yet. Black moves first.",
  outcomeWinNoMoves: "wins (no legal moves left)!",
  outcomeWinResign: "wins by resignation!",
  outcomeWinTimeout: "wins on time!",
  outcomeDrawAgreement: "Draw by mutual agreement.",
  outcomeDrawRepetition: "Draw by threefold repetition.",
  outcomeDrawFortyMove: "Draw by 40-move rule.",
  drawDeclinedError: "Computer declined your draw offer.",
  poolOpenTable: "Open Table",
  poolSolids: "Solids (1–7)",
  poolStripes: "Stripes (9–15)",
  poolShootEight: "8-Ball!",
  poolBreakHint: "Break shot — pull back anywhere like a slingshot to aim & shoot (drag cue ball to place)!",
  poolBallInHand: "Ball in hand — drag cue ball anywhere, then pull back like a slingshot to shoot.",
  poolBallInHandBehind: "Ball in hand — drag cue ball behind the head string, then pull back to shoot.",
  poolCallPocketHint: "Click a pocket to call your 8-ball shot.",
  poolShootBtn: "Shoot! (Space)",
  poolRerackBtn: "Re-Rack Table",
  poolSpinLabel: "English (Spin)",
  poolSpinCenter: "Center",
  poolPowerLabel: "Power",
  poolFineAimLabel: "Fine Aim",
  poolAimAidLabel: "Aiming Aid",
  poolAimOff: "Off",
  poolAimBasic: "Basic",
  poolAimExtended: "Extended",
  poolTimerLabel: "Shot Clock",
  poolCallEightLabel: "Call 8-Ball Pocket",
  poolFoulScratch: "Foul: Scratch (cue ball pocketed)!",
  poolFoulWrongBall: "Foul: Wrong ball hit first!",
  poolFoulNoHit: "Foul: No object ball hit!",
  poolFoulNoRail: "Foul: No rail after contact!",
  poolWinEight: "wins by pocketing the 8-ball!",
  poolWinEarlyEight: "wins (opponent sank the 8-ball early)!",
  poolWinFoulEight: "wins (opponent fouled on the 8-ball)!",
  poolWinResign: "wins by resignation!",
  poolRerackedNotice: "8-ball on the break — rack reset!",
};

const DE: Strings = {
  subtitle: "Klassische Offline-Lounge",
  offlineBadge: "Offline WASM",
  highContrast: "Hoher Kontrast",
  soundOn: "Ton: An",
  soundOff: "Ton: Aus",
  skipToContent: "Zum Hauptinhalt springen",
  landscapeHint: "Tipp: Querformat bietet auf Smartphones einen breiteren Billardtisch.",
  poolKeysHint: "Tasten: ←/→ zielen (Shift = fein), ↑/↓ Stärke, Leertaste = Stoß, Esc = Abbruch.",
  checkersKeysHint: "Tasten: Pfeile bewegen Cursor, Enter/Leertaste wählen, Esc abbrechen.",
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
  footerNote: "100 % kostenlos · Keine Cookies · Läuft lokal in Rust/WebAssembly",
  player1: "Spieler 1",
  player2: "Spieler 2",
  computer: "Computer",
  colorBlack: "Schwarz",
  colorWhite: "Rot",
  yourTurn: "Am Zug",
  botThinking: "Computer überlegt…",
  mustCapture: "Schlagzwang!",
  captured: "Geschlagen",
  flipBoard: "Brett drehen",
  showNumbers: "Felder 1–32",
  offerDraw: "Remis anbieten",
  resign: "Aufgeben",
  rematch: "Revanche (Wechsel)",
  drawOfferedPrompt: "Remis angeboten — annehmen?",
  acceptDraw: "Remis annehmen",
  declineDraw: "Ablehnen",
  clockPreset: "Uhr",
  clockUnlimited: "Ohne Uhr",
  clockMove30: "30 s / Zug",
  clockGame5: "5 Min. Partie",
  clockGame10: "10 Min. Partie",
  moveListTitle: "Zugliste",
  noMovesYet: "Noch keine Züge. Schwarz beginnt.",
  outcomeWinNoMoves: "gewinnt (keine legalen Züge)!",
  outcomeWinResign: "gewinnt durch Aufgabe!",
  outcomeWinTimeout: "gewinnt auf Zeit!",
  outcomeDrawAgreement: "Unentschieden durch Einigung.",
  outcomeDrawRepetition: "Unentschieden durch dreifache Stellungswiederholung.",
  outcomeDrawFortyMove: "Unentschieden nach der 40-Züge-Regel.",
  drawDeclinedError: "Der Computer hat das Remis-Angebot abgelehnt.",
  poolOpenTable: "Offener Tisch",
  poolSolids: "Volle (1–7)",
  poolStripes: "Halbe (9–15)",
  poolShootEight: "Auf die 8!",
  poolBreakHint: "Anstoß — irgendwo zurückziehen wie eine Schleuder zum Zielen & Stoßen (Weiße verschiebbar)!",
  poolBallInHand: "Ball in Hand — Weiße verschieben, dann wie eine Schleuder zurückziehen & loslassen.",
  poolBallInHandBehind: "Ball in Hand — Weiße hinter der Kopflinie platzieren, dann zurückziehen & loslassen.",
  poolCallPocketHint: "Klicke auf eine Tasche, um die 8 anzusagen.",
  poolShootBtn: "Stoß! (Leertaste)",
  poolRerackBtn: "Neu aufbauen",
  poolSpinLabel: "Effet (Spin)",
  poolSpinCenter: "Mitte",
  poolPowerLabel: "Stärke",
  poolFineAimLabel: "Feinjustierung",
  poolAimAidLabel: "Zielhilfe",
  poolAimOff: "Aus",
  poolAimBasic: "Einfach",
  poolAimExtended: "Erweitert",
  poolTimerLabel: "Stoßuhr",
  poolCallEightLabel: "8-Ball Tasche ansagen",
  poolFoulScratch: "Foul: Weiße gefallen!",
  poolFoulWrongBall: "Foul: Falsche Kugel zuerst getroffen!",
  poolFoulNoHit: "Foul: Keine Kugel getroffen!",
  poolFoulNoRail: "Foul: Keine Bande nach Kontakt!",
  poolWinEight: "gewinnt mit der 8!",
  poolWinEarlyEight: "gewinnt (Gegner hat die 8 zu früh versenkt)!",
  poolWinFoulEight: "gewinnt (Foul des Gegners auf der 8)!",
  poolWinResign: "gewinnt durch Aufgabe!",
  poolRerackedNotice: "8 beim Anstoß gefallen — neu aufgebaut!",
};

export function getStrings(lang: string): Strings {
  return lang === "de" ? DE : EN;
}
