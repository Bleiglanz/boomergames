export interface JazzTrack {
  readonly id: string;
  readonly title: string;
  readonly src: string;
  readonly sourceUrl: string;
}

export const JAZZ_TRACKS: readonly JazzTrack[] = [
  {
    id: "backbay-lounge",
    title: "Backbay Lounge",
    src: "./audio/backbay-lounge.mp3",
    sourceUrl: "https://incompetech.com/music/royalty-free/index.html?isrc=USUAN1700068",
  },
  {
    id: "airport-lounge",
    title: "Airport Lounge",
    src: "./audio/airport-lounge.mp3",
    sourceUrl: "https://incompetech.com/music/royalty-free/index.html?isrc=USUAN1100806",
  },
  {
    id: "cool-vibes",
    title: "Cool Vibes",
    src: "./audio/cool-vibes.mp3",
    sourceUrl: "https://incompetech.com/music/royalty-free/index.html?isrc=USUAN1100863",
  },
];

// Keep lounge jazz gently tucked underneath billiard & checkers SFX.
const MUSIC_GAIN_SCALE = 0.42;

/**
 * Lazy-loading background lounge jazz player.
 * Fetches zero audio bytes until the user explicitly enables music.
 */
export class MusicPlayer {
  private audio: HTMLAudioElement | null = null;
  private enabled: boolean;
  private muted: boolean;
  private volume: number;
  private trackIndex: number;
  private readonly listeners = new Set<() => void>();

  public constructor(enabled: boolean, muted: boolean, volume: number, trackIndex = 0) {
    this.enabled = enabled;
    this.muted = muted;
    this.volume = Math.max(0, Math.min(1, volume));
    this.trackIndex = ((trackIndex % JAZZ_TRACKS.length) + JAZZ_TRACKS.length) % JAZZ_TRACKS.length;
  }

  public subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }

  public isEnabled(): boolean {
    return this.enabled;
  }

  public currentTrackIndex(): number {
    return this.trackIndex;
  }

  public currentTrack(): JazzTrack {
    return JAZZ_TRACKS[this.trackIndex] ?? JAZZ_TRACKS[0]!;
  }

  public setEnabled(enabled: boolean): void {
    this.enabled = enabled;
    if (enabled) {
      this.ensureAudioAndPlay();
    } else if (this.audio !== null) {
      this.audio.pause();
    }
    this.notify();
  }

  public nextTrack(): void {
    this.trackIndex = (this.trackIndex + 1) % JAZZ_TRACKS.length;
    if (this.audio !== null) {
      this.audio.src = this.currentTrack().src;
      this.audio.currentTime = 0;
      if (this.enabled && !this.muted && this.volume > 0.001) {
        void this.audio.play().catch(() => {});
      }
    } else if (this.enabled) {
      this.ensureAudioAndPlay();
    }
    this.notify();
  }

  public setMuted(muted: boolean): void {
    this.muted = muted;
    this.syncAudioLevel();
  }

  public setVolume(volume: number): void {
    this.volume = Math.max(0, Math.min(1, volume));
    this.syncAudioLevel();
  }

  /** Called on user gestures so persisted `musicEnabled: true` starts smoothly past autoplay gates. */
  public unlock(): void {
    if (!this.enabled || this.muted || this.volume <= 0.001) {
      return;
    }
    if (this.audio === null || this.audio.paused) {
      this.ensureAudioAndPlay();
    }
  }

  private ensureAudioAndPlay(): void {
    if (this.audio === null) {
      const el = new Audio();
      el.preload = "auto";
      el.src = this.currentTrack().src;
      el.addEventListener("ended", () => {
        this.nextTrack();
      });
      this.audio = el;
    }
    this.syncAudioLevel();
    if (!this.muted && this.volume > 0.001) {
      void this.audio.play().catch(() => {
        // Browser autoplay gate will allow playback on next user gesture via unlock().
      });
    }
  }

  private syncAudioLevel(): void {
    if (this.audio === null) {
      return;
    }
    const effective = this.muted ? 0 : Math.min(1, this.volume * MUSIC_GAIN_SCALE);
    this.audio.volume = effective;
    if (this.enabled && !this.muted && effective > 0.001) {
      if (this.audio.paused) {
        void this.audio.play().catch(() => {});
      }
    } else if (!this.audio.paused) {
      this.audio.pause();
    }
  }

  private notify(): void {
    for (const fn of this.listeners) {
      fn();
    }
  }
}
