export const THEME_IDS = [
  "classic",
  "parlor",
  "midnight",
  "sepia",
  "retro98",
  "high_contrast",
] as const;

export type ThemeId = (typeof THEME_IDS)[number];

/**
 * Strictly local-only preferences persisted in `localStorage` (zero cookies per NFR-8).
 */
export interface UserPrefs {
  readonly lang: "en" | "de";
  readonly theme: ThemeId;
  readonly highContrast: boolean;
  readonly muted: boolean;
  readonly volume: number;
  readonly musicEnabled: boolean;
  readonly musicTrack: number;
  readonly checkersShowNumbers: boolean;
  readonly chessShowCoords: boolean;
}

const STORAGE_KEY = "boomergames.prefs.v1";

const DEFAULT_PREFS: UserPrefs = {
  lang: "en",
  theme: "classic",
  highContrast: false,
  muted: false,
  volume: 0.75,
  musicEnabled: false,
  musicTrack: 0,
  checkersShowNumbers: false,
  chessShowCoords: true,
};

function parseTheme(raw: unknown, legacyHighContrast: unknown): ThemeId {
  if (typeof raw === "string" && (THEME_IDS as readonly string[]).includes(raw)) {
    return raw as ThemeId;
  }
  return legacyHighContrast === true ? "high_contrast" : "classic";
}

function clampVolume(raw: unknown): number {
  if (typeof raw !== "number" || !Number.isFinite(raw)) {
    return DEFAULT_PREFS.volume;
  }
  return Math.max(0, Math.min(1, raw));
}

export function loadPrefs(): UserPrefs {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (raw === null) {
      return DEFAULT_PREFS;
    }
    const parsed = JSON.parse(raw) as Partial<UserPrefs>;
    const theme = parseTheme(parsed.theme, parsed.highContrast);
    return {
      lang: parsed.lang === "de" ? "de" : "en",
      theme,
      highContrast: theme === "high_contrast",
      muted: parsed.muted === true,
      volume: clampVolume(parsed.volume),
      musicEnabled: parsed.musicEnabled === true,
      musicTrack:
        typeof parsed.musicTrack === "number" && Number.isInteger(parsed.musicTrack) && parsed.musicTrack >= 0
          ? parsed.musicTrack
          : 0,
      checkersShowNumbers: parsed.checkersShowNumbers === true,
      chessShowCoords: parsed.chessShowCoords !== false,
    };
  } catch {
    return DEFAULT_PREFS;
  }
}

export function savePrefs(prefs: UserPrefs): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // Ignore storage quota or private-mode restrictions cleanly.
  }
}
