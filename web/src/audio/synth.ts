import type { SoundCue } from "../wasm_bridge.js";

/**
 * Lightweight zero-asset Web Audio synthesizer producing short, era-appropriate
 * 2000s billiard and checkers sound effects.
 */
export class SoundSynth {
  private ctx: AudioContext | null = null;
  private muted: boolean;
  private lastClickMs = 0;

  public constructor(muted: boolean) {
    this.muted = muted;
  }

  public isMuted(): boolean {
    return this.muted;
  }

  public setMuted(muted: boolean): void {
    this.muted = muted;
  }

  public unlock(): void {
    if (this.muted) {
      return;
    }
    const ctx = this.ensureContext();
    if (ctx !== null && ctx.state === "suspended") {
      void ctx.resume();
    }
  }

  public playAll(cues: readonly SoundCue[]): void {
    if (this.muted || cues.length === 0) {
      return;
    }
    const ctx = this.ensureContext();
    if (ctx === null) {
      return;
    }
    if (ctx.state === "suspended") {
      void ctx.resume();
    }
    for (const cue of cues) {
      this.playOne(ctx, cue);
    }
  }

  private ensureContext(): AudioContext | null {
    if (this.ctx !== null) {
      return this.ctx;
    }
    const Ctor = window.AudioContext;
    if (typeof Ctor !== "function") {
      return null;
    }
    this.ctx = new Ctor();
    return this.ctx;
  }

  private playOne(ctx: AudioContext, cue: SoundCue): void {
    const now = ctx.currentTime;
    switch (cue.kind) {
      case "cue_hit":
        this.playTone(ctx, now, "triangle", 260, 110, 0.055, 0.22 * cue.intensity);
        this.playNoiseBurst(ctx, now, 0.03, 1800, 0.14 * cue.intensity);
        break;
      case "ball_click": {
        // Light rate-limit on dense rack breaks so 15-ball scatter sounds crisp rather than clipping.
        const perfNow = performance.now();
        if (perfNow - this.lastClickMs < 18) {
          return;
        }
        this.lastClickMs = perfNow;
        this.playTone(ctx, now, "sine", 2650, 1950, 0.028, 0.16 * cue.intensity);
        this.playTone(ctx, now, "triangle", 4100, 3200, 0.018, 0.09 * cue.intensity);
        break;
      }
      case "cushion_thud":
        this.playTone(ctx, now, "sine", 145, 68, 0.06, 0.2 * cue.intensity);
        break;
      case "pocket_drop":
        this.playTone(ctx, now, "triangle", 190, 95, 0.075, 0.22);
        this.playTone(ctx, now + 0.06, "triangle", 230, 115, 0.065, 0.15);
        break;
      case "checker_move":
        this.playTone(ctx, now, "triangle", 320, 210, 0.045, 0.16);
        break;
      case "checker_capture":
        this.playTone(ctx, now, "triangle", 420, 290, 0.05, 0.18);
        this.playTone(ctx, now + 0.055, "triangle", 540, 380, 0.055, 0.18);
        break;
      case "checker_king":
        this.playTone(ctx, now, "sine", 523.25, 523.25, 0.09, 0.16);
        this.playTone(ctx, now + 0.08, "sine", 659.25, 659.25, 0.09, 0.16);
        this.playTone(ctx, now + 0.16, "sine", 783.99, 783.99, 0.15, 0.18);
        break;
      case "clock_warn":
        this.playTone(ctx, now, "sine", 880, 880, 0.04, 0.11);
        break;
      case "game_over":
        this.playTone(ctx, now, "sine", 392.0, 392.0, 0.18, 0.14);
        this.playTone(ctx, now + 0.12, "sine", 493.88, 493.88, 0.2, 0.14);
        this.playTone(ctx, now + 0.24, "sine", 587.33, 587.33, 0.28, 0.16);
        break;
    }
  }

  private playTone(
    ctx: AudioContext,
    startTime: number,
    type: OscillatorType,
    startFreq: number,
    endFreq: number,
    durationSec: number,
    peakGain: number,
  ): void {
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = type;
    osc.frequency.setValueAtTime(startFreq, startTime);
    if (endFreq !== startFreq) {
      osc.frequency.exponentialRampToValueAtTime(Math.max(20, endFreq), startTime + durationSec);
    }
    gain.gain.setValueAtTime(0.0001, startTime);
    gain.gain.linearRampToValueAtTime(Math.max(0.001, peakGain), startTime + 0.004);
    gain.gain.exponentialRampToValueAtTime(0.0001, startTime + durationSec);
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.start(startTime);
    osc.stop(startTime + durationSec + 0.01);
  }

  private playNoiseBurst(
    ctx: AudioContext,
    startTime: number,
    durationSec: number,
    filterHz: number,
    peakGain: number,
  ): void {
    const sampleCount = Math.max(1, Math.floor(ctx.sampleRate * durationSec));
    const buffer = ctx.createBuffer(1, sampleCount, ctx.sampleRate);
    const data = buffer.getChannelData(0);
    for (let i = 0; i < sampleCount; i += 1) {
      data[i] = (Math.random() * 2 - 1) * (1 - i / sampleCount);
    }
    const src = ctx.createBufferSource();
    src.buffer = buffer;

    const filter = ctx.createBiquadFilter();
    filter.type = "bandpass";
    filter.frequency.setValueAtTime(filterHz, startTime);

    const gain = ctx.createGain();
    gain.gain.setValueAtTime(Math.max(0.001, peakGain), startTime);
    gain.gain.exponentialRampToValueAtTime(0.0001, startTime + durationSec);

    src.connect(filter);
    filter.connect(gain);
    gain.connect(ctx.destination);
    src.start(startTime);
    src.stop(startTime + durationSec + 0.01);
  }
}
