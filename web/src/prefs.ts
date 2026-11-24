/**
 * Strictly local-only preferences persisted in `localStorage` (zero cookies per NFR-8).
 */
export interface UserPrefs {
  readonly lang: "en" | "de";
  readonly highContrast: boolean;
  readonly muted: boolean;
  readonly checkersShowNumbers: boolean;
}

const STORAGE_KEY = "boomergames.prefs.v1";

const DEFAULT_PREFS: UserPrefs = {
  lang: "en",
  highContrast: false,
  muted: false,
  checkersShowNumbers: false,
};

export function loadPrefs(): UserPrefs {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (raw === null) {
      return DEFAULT_PREFS;
    }
    const parsed = JSON.parse(raw) as Partial<UserPrefs>;
    return {
      lang: parsed.lang === "de" ? "de" : "en",
      highContrast: parsed.highContrast === true,
      muted: parsed.muted === true,
      checkersShowNumbers: parsed.checkersShowNumbers === true,
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
