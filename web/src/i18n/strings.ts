export type Lang = "en" | "de";

export interface Strings {
  readonly subtitle: string;
  readonly offlineBadge: string;
  readonly highContrast: string;
  readonly soundOn: string;
  readonly soundOff: string;
  readonly soundVolume: string;
  readonly skipToContent: string;
  readonly landscapeHint: string;
  readonly poolKeysHint: string;
  readonly checkersKeysHint: string;
  readonly checkersFullscreenHint: string;
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
  readonly privacyLink: string;
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
  readonly poolFullscreenBtn: string;
  readonly poolExitFullscreenBtn: string;
  readonly poolBehindCueBtn: string;
  readonly poolReset2DBtn: string;
  readonly poolFullscreenHint: string;
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
  // Online Lounge & Table strings (Milestone 5)
  readonly navOnlineLounge: string;
  readonly navOfflinePlay: string;
  readonly loungeOfflineTitle: string;
  readonly loungeOfflineDesc: string;
  readonly loungePlayOfflineBtn: string;
  readonly loungeRetryBtn: string;
  readonly loungeConnecting: string;
  readonly loungeRoomLabel: string;
  readonly loungePlayNow: string;
  readonly loungeCreateTable: string;
  readonly loungeHighscores: string;
  readonly loungeSignIn: string;
  readonly loungeAccountTitle: string;
  readonly loungeSignInDesc: string;
  readonly loungeSendMagicLink: string;
  readonly loungeOrDivider: string;
  readonly loungeGoogleLoading: string;
  readonly loungeMagicSent: string;
  readonly loungeSignedIn: string;
  readonly loungeNicknameLabel: string;
  readonly loungeNicknameHint: string;
  readonly loungeSaveAccount: string;
  readonly loungeLogout: string;
  readonly loungeDeleteAccount: string;
  readonly loungeDeleteConfirm: string;
  readonly loungeCancel: string;
  readonly loungeTablesTab: string;
  readonly loungePlayersTab: string;
  readonly loungeChatTab: string;
  readonly loungeTableTab: string;
  readonly loungeControlsTab: string;
  readonly loungeRoomChatTitle: string;
  readonly loungeTableChatTitle: string;
  readonly loungeChatEmpty: string;
  readonly loungeChatPlaceholder: string;
  readonly loungeSendBtn: string;
  readonly loungeNoTables: string;
  readonly loungeOpenSeat: string;
  readonly loungeRated: string;
  readonly loungeUnrated: string;
  readonly loungeJoin: string;
  readonly loungeWatch: string;
  readonly loungeFull: string;
  readonly loungeInviteBtn: string;
  readonly loungeIgnore: string;
  readonly loungeUnignore: string;
  readonly loungeInviteFrom: string;
  readonly loungeAccept: string;
  readonly loungeDecline: string;
  readonly loungePlayNowWaiting: string;
  readonly loungePlayNowTimeout: string;
  readonly loungeCopyLink: string;
  readonly loungeLinkCopied: string;
  readonly loungeLeaveTable: string;
  readonly loungeSeat: string;
  readonly loungeReadyBadge: string;
  readonly loungeNotReadyBadge: string;
  readonly loungeWaitingPlayer: string;
  readonly loungeSitDown: string;
  readonly loungeStandUp: string;
  readonly loungeReadyBtn: string;
  readonly loungeUnreadyBtn: string;
  readonly loungeBoot: string;
  readonly loungeMuteSpectators: string;
  readonly loungeUnmuteSpectators: string;
  readonly loungeSpectatorsMuted: string;
  readonly loungeSpectatorsOpen: string;
  readonly loungeRemoveBot: string;
  readonly loungeSpectators: string;
  readonly loungeNoSpectators: string;
  readonly loungeVisPublic: string;
  readonly loungeVisProtected: string;
  readonly loungeVisPrivate: string;
  readonly loungeCreateConfirm: string;
  readonly loungeFindMe: string;
  readonly loungeNoHighscores: string;
  readonly loungeProfileTitle: string;
  readonly loungeProfileLoading: string;
  readonly loungeProfileSince: string;
  readonly loungeProfileStats: string;
  readonly loungeProfileRecent: string;
  readonly loungeProfileNoMatches: string;
  readonly loungeProfileView: string;
  // Live Online Match strings
  readonly onlineBadge: string;
  readonly onlineMatch: string;
  readonly yourTurnBanner: string;
  readonly opponentTurnBanner: string;
  readonly spectatingBanner: string;
  readonly drawOfferSent: string;
  readonly rematchWaiting: string;
  readonly opponentDisconnected: string;
}

const EN: Strings = {
  subtitle: "Classic Offline Lounge",
  offlineBadge: "Offline WASM",
  highContrast: "High Contrast",
  soundOn: "Sound: On",
  soundOff: "Sound: Off",
  soundVolume: "Sound Volume",
  skipToContent: "Skip to main content",
  landscapeHint: "Tip: Rotate your phone to landscape for a wider pool table.",
  poolKeysHint: "Keys: ←/→ aim (Shift = fine), ↑/↓ power, Space = shoot, Esc = cancel.",
  checkersKeysHint: "Keys: Arrows move cursor, Enter/Space select, Esc clear.",
  checkersFullscreenHint: "Click/drag: move · Right/Shift-drag / Scroll / WASD: 3D view · R: 2D · F: exit",
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
  privacyLink: "Privacy Policy",
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
  poolFullscreenBtn: "⛶ Full Screen (F)",
  poolExitFullscreenBtn: "✕ Exit Full Screen",
  poolBehindCueBtn: "🎯 Behind Cue (C)",
  poolReset2DBtn: "↺ Top-Down 2D",
  poolFullscreenHint: "Left-drag: shoot · Right/Shift-drag / Scroll / WASD: view · C: cue cam · R: 2D · F: exit",
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
  navOnlineLounge: "Online Lounge",
  navOfflinePlay: "Offline Play",
  loungeOfflineTitle: "Online Lounge Unavailable Here",
  loungeOfflineDesc:
    "This page is running as a static offline build (no live multiplayer server). You can play Checkers and 8-Ball Pool offline against the computer or hotseat right now!",
  loungePlayOfflineBtn: "Play Offline Games",
  loungeRetryBtn: "Retry Connection",
  loungeConnecting: "Connecting to Boomergames lounge…",
  loungeRoomLabel: "Room",
  loungePlayNow: "Play Now",
  loungeCreateTable: "Create Table",
  loungeHighscores: "High Scores",
  loungeSignIn: "Sign In",
  loungeAccountTitle: "Account & Avatar",
  loungeSignInDesc:
    "Enter your email for a passwordless magic login link (no password, no cookies).",
  loungeSendMagicLink: "Send Login Link",
  loungeOrDivider: "or sign in with email",
  loungeGoogleLoading: "Loading Google Sign-In…",
  loungeMagicSent: "If the email is valid, your login link has been sent (valid 15 min).",
  loungeSignedIn: "Signed in! Keep your last nickname or pick a new one:",
  loungeNicknameLabel: "Nickname (shown in lounge & games)",
  loungeNicknameHint: "Saved to your email — you can change it on every login (3–20 letters, numbers, _ or -).",
  loungeSaveAccount: "Save & Play",
  loungeLogout: "Sign Out",
  loungeDeleteAccount: "Delete Account",
  loungeDeleteConfirm: "Permanently delete your account and stats? This cannot be undone.",
  loungeCancel: "Close",
  loungeTablesTab: "Tables",
  loungePlayersTab: "Players",
  loungeChatTab: "Chat",
  loungeTableTab: "Table",
  loungeControlsTab: "Host & Info",
  loungeRoomChatTitle: "Room Chat",
  loungeTableChatTitle: "Table Chat",
  loungeChatEmpty: "No messages yet. Say hello!",
  loungeChatPlaceholder: "Type a message (max 300 chars)…",
  loungeSendBtn: "Send",
  loungeNoTables: "No open tables yet. Click Create Table or Play Now!",
  loungeOpenSeat: "Open",
  loungeRated: "Rated",
  loungeUnrated: "Unrated",
  loungeJoin: "Join",
  loungeWatch: "Watch",
  loungeFull: "Full",
  loungeInviteBtn: "Invite",
  loungeIgnore: "Ignore",
  loungeUnignore: "Unignore",
  loungeInviteFrom: "Table invite from",
  loungeAccept: "Accept",
  loungeDecline: "Decline",
  loungePlayNowWaiting: "Looking for a human opponent nearby in rating…",
  loungePlayNowTimeout: "No human opponent matched yet — start a bot match right away?",
  loungeCopyLink: "Copy Link",
  loungeLinkCopied: "Link Copied!",
  loungeLeaveTable: "Leave Table",
  loungeSeat: "Seat",
  loungeReadyBadge: "Ready",
  loungeNotReadyBadge: "Not ready",
  loungeWaitingPlayer: "Waiting for player…",
  loungeSitDown: "Sit Down",
  loungeStandUp: "Stand Up",
  loungeReadyBtn: "Ready / Start",
  loungeUnreadyBtn: "Cancel Ready",
  loungeBoot: "Boot",
  loungeMuteSpectators: "Mute Spectators",
  loungeUnmuteSpectators: "Unmute Spectators",
  loungeSpectatorsMuted: "Spectators muted",
  loungeSpectatorsOpen: "Spectator chat on",
  loungeRemoveBot: "Remove Bot",
  loungeSpectators: "Spectators",
  loungeNoSpectators: "No spectators watching.",
  loungeVisPublic: "Public (anyone can join)",
  loungeVisProtected: "Protected (invite to play)",
  loungeVisPrivate: "Private (invite only)",
  loungeCreateConfirm: "Create Table",
  loungeFindMe: "Find Me",
  loungeNoHighscores: "No ranked human wins recorded yet.",
  loungeProfileTitle: "Player Profile",
  loungeProfileLoading: "Loading player profile…",
  loungeProfileSince: "Member since",
  loungeProfileStats: "Ratings & Record",
  loungeProfileRecent: "Recent Matches",
  loungeProfileNoMatches: "No finished matches recorded yet.",
  loungeProfileView: "Profile",
  onlineBadge: "Online",
  onlineMatch: "Online Match",
  yourTurnBanner: "Your turn!",
  opponentTurnBanner: "Waiting for opponent…",
  spectatingBanner: "Spectating live match",
  drawOfferSent: "Draw offer sent — waiting for opponent…",
  rematchWaiting: "Rematch requested — waiting for opponent…",
  opponentDisconnected: "Opponent disconnected — waiting up to 45 s to reconnect…",
};

const DE: Strings = {
  subtitle: "Klassische Offline-Lounge",
  offlineBadge: "Offline WASM",
  highContrast: "Hoher Kontrast",
  soundOn: "Ton: An",
  soundOff: "Ton: Aus",
  soundVolume: "Lautstärke",
  skipToContent: "Zum Hauptinhalt springen",
  landscapeHint: "Tipp: Querformat bietet auf Smartphones einen breiteren Billardtisch.",
  poolKeysHint: "Tasten: ←/→ zielen (Shift = fein), ↑/↓ Stärke, Leertaste = Stoß, Esc = Abbruch.",
  checkersKeysHint: "Tasten: Pfeile bewegen Cursor, Enter/Leertaste wählen, Esc abbrechen.",
  checkersFullscreenHint: "Klick/Ziehen: Zug · Rechts/Shift / Rad / WASD: 3D-Ansicht · R: 2D · F: zurück",
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
  privacyLink: "Datenschutz",
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
  poolFullscreenBtn: "⛶ Vollbild (F)",
  poolExitFullscreenBtn: "✕ Vollbild beenden",
  poolBehindCueBtn: "🎯 Queue-Sicht (C)",
  poolReset2DBtn: "↺ 2D Draufsicht",
  poolFullscreenHint: "Links ziehen: Stoß · Rechts/Shift / Rad / WASD: Ansicht · C: Queue · R: 2D · F: zurück",
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
  navOnlineLounge: "Online-Lounge",
  navOfflinePlay: "Offline spielen",
  loungeOfflineTitle: "Online-Lounge hier nicht verfügbar",
  loungeOfflineDesc:
    "Diese Seite läuft als statische Offline-Version (ohne Live-Server). Dame und 8-Ball Billard sind direkt offline gegen den Computer oder lokal zu zweit spielbar!",
  loungePlayOfflineBtn: "Offline spielen",
  loungeRetryBtn: "Erneut verbinden",
  loungeConnecting: "Verbinde mit der Boomergames-Lounge…",
  loungeRoomLabel: "Raum",
  loungePlayNow: "Jetzt spielen",
  loungeCreateTable: "Tisch erstellen",
  loungeHighscores: "Bestenliste",
  loungeSignIn: "Anmelden",
  loungeAccountTitle: "Konto & Avatar",
  loungeSignInDesc:
    "Gib deine E-Mail-Adresse für einen passwortlosen Login-Link ein (kein Passwort, keine Cookies).",
  loungeSendMagicLink: "Login-Link senden",
  loungeOrDivider: "oder per E-Mail anmelden",
  loungeGoogleLoading: "Google-Anmeldung wird geladen…",
  loungeMagicSent: "Falls die Adresse gültig ist, wurde dein Login-Link gesendet (15 Min. gültig).",
  loungeSignedIn: "Angemeldet! Behalte deinen Spitznamen oder wähle einen neuen:",
  loungeNicknameLabel: "Spitzname (in Lounge & Spielen sichtbar)",
  loungeNicknameHint: "Mit deiner E-Mail verknüpft — bei jedem Login änderbar (3–20 Buchstaben, Zahlen, _ oder -).",
  loungeSaveAccount: "Speichern & Spielen",
  loungeLogout: "Abmelden",
  loungeDeleteAccount: "Konto löschen",
  loungeDeleteConfirm: "Konto und Statistiken dauerhaft löschen? Dies kann nicht rückgängig gemacht werden.",
  loungeCancel: "Schließen",
  loungeTablesTab: "Tische",
  loungePlayersTab: "Spieler",
  loungeChatTab: "Chat",
  loungeTableTab: "Tisch",
  loungeControlsTab: "Host & Info",
  loungeRoomChatTitle: "Raum-Chat",
  loungeTableChatTitle: "Tisch-Chat",
  loungeChatEmpty: "Noch keine Nachrichten. Sag Hallo!",
  loungeChatPlaceholder: "Nachricht schreiben (max. 300 Zeichen)…",
  loungeSendBtn: "Senden",
  loungeNoTables: "Noch keine offenen Tische. Erstelle einen Tisch oder nutze Jetzt spielen!",
  loungeOpenSeat: "Frei",
  loungeRated: "Gewertet",
  loungeUnrated: "Ungewertet",
  loungeJoin: "Beitreten",
  loungeWatch: "Zusehen",
  loungeFull: "Voll",
  loungeInviteBtn: "Einladen",
  loungeIgnore: "Ignorieren",
  loungeUnignore: "Nicht ignorieren",
  loungeInviteFrom: "Tischeinladung von",
  loungeAccept: "Annehmen",
  loungeDecline: "Ablehnen",
  loungePlayNowWaiting: "Suche einen passenden Gegner in deiner Wertung…",
  loungePlayNowTimeout: "Noch kein menschlicher Gegner gefunden — direkt gegen den Computer spielen?",
  loungeCopyLink: "Link kopieren",
  loungeLinkCopied: "Kopiert!",
  loungeLeaveTable: "Tisch verlassen",
  loungeSeat: "Platz",
  loungeReadyBadge: "Bereit",
  loungeNotReadyBadge: "Wartet",
  loungeWaitingPlayer: "Wartet auf Spieler…",
  loungeSitDown: "Hinsetzen",
  loungeStandUp: "Aufstehen",
  loungeReadyBtn: "Bereit / Start",
  loungeUnreadyBtn: "Nicht bereit",
  loungeBoot: "Entfernen",
  loungeMuteSpectators: "Zuschauer stumm",
  loungeUnmuteSpectators: "Zuschauer erlauben",
  loungeSpectatorsMuted: "Zuschauer stummgeschaltet",
  loungeSpectatorsOpen: "Zuschauer-Chat aktiv",
  loungeRemoveBot: "Bot entfernen",
  loungeSpectators: "Zuschauer",
  loungeNoSpectators: "Keine Zuschauer am Tisch.",
  loungeVisPublic: "Öffentlich (alle dürfen spielen)",
  loungeVisProtected: "Geschützt (Spielen per Einladung)",
  loungeVisPrivate: "Privat (nur mit Einladung)",
  loungeCreateConfirm: "Tisch eröffnen",
  loungeFindMe: "Finde mich",
  loungeNoHighscores: "Noch keine gewerteten Siege gegen Menschen.",
  loungeProfileTitle: "Spielerprofil",
  loungeProfileLoading: "Lade Spielerprofil…",
  loungeProfileSince: "Mitglied seit",
  loungeProfileStats: "Wertungen & Bilanz",
  loungeProfileRecent: "Letzte Partien",
  loungeProfileNoMatches: "Noch keine abgeschlossenen Partien.",
  loungeProfileView: "Profil",
  onlineBadge: "Online",
  onlineMatch: "Online-Partie",
  yourTurnBanner: "Du bist am Zug!",
  opponentTurnBanner: "Warte auf Gegner…",
  spectatingBanner: "Live-Partie (Zuschauer)",
  drawOfferSent: "Remis angeboten — warte auf Antwort…",
  rematchWaiting: "Revanche angefragt — warte auf Gegner…",
  opponentDisconnected: "Gegner getrennt — warte bis zu 45 s auf Wiederverbindung…",
};

export function getStrings(lang: string): Strings {
  return lang === "de" ? DE : EN;
}
