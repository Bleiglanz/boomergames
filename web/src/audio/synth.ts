import type { SoundCue } from "../wasm_bridge.js";

/**
 * Lightweight zero-asset Web Audio synthesizer producing tactile, era-appropriate
 * billiard and hardwood checkers acoustics with master volume & soft limiting.
 */
export class SoundSynth {
  private ctx: AudioContext | null = null;
  private masterGain: GainNode | null = null;
  private noiseBuffer: AudioBuffer | null = null;
  private muted: boolean;
  private volume: number;
  private lastClickMs = 0;

  public constructor(muted: boolean, volume = 0.75) {
    this.muted = muted;
    this.volume = Math.max(0, Math.min(1, volume));
  }

  public isMuted(): boolean {
    return this.muted;
  }

  public setMuted(muted: boolean): void {
    this.muted = muted;
    this.syncMasterGain();
  }

  public getVolume(): number {
    return this.volume;
  }

  public setVolume(volume: number): void {
    this.volume = Math.max(0, Math.min(1, volume));
    this.syncMasterGain();
  }

  public unlock(): void {
    if (this.muted || this.volume <= 0.001) {
      return;
    }
    const ctx = this.ensureContext();
    if (ctx !== null && ctx.state === "suspended") {
      void ctx.resume();
    }
  }

  public playAll(cues: readonly SoundCue[]): void {
    if (this.muted || this.volume <= 0.001 || cues.length === 0) {
      return;
    }
    const ctx = this.ensureContext();
    if (ctx === null || this.masterGain === null) {
      return;
    }
    if (ctx.state === "suspended") {
      void ctx.resume();
    }
    for (const cue of cues) {
      this.playOne(ctx, this.masterGain, cue);
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
    const ctx = new Ctor();
    const master = ctx.createGain();
    master.gain.value = this.effectiveGain();

    // Gentle dynamics compressor prevents clipping on dense 15-ball break scatters.
    const comp = ctx.createDynamicsCompressor();
    comp.threshold.value = -10;
    comp.knee.value = 12;
    comp.ratio.value = 6;
    comp.attack.value = 0.002;
    comp.release.value = 0.08;

    master.connect(comp);
    comp.connect(ctx.destination);

    // Pre-generate a reusable 0.2s white noise buffer for transient wood/chalk bursts.
    const len = Math.max(1, Math.floor(ctx.sampleRate * 0.2));
    const buf = ctx.createBuffer(1, len, ctx.sampleRate);
    const data = buf.getChannelData(0);
    for (let i = 0; i < len; i += 1) {
      data[i] = Math.random() * 2 - 1;
    }

    this.ctx = ctx;
    this.masterGain = master;
    this.noiseBuffer = buf;
    return ctx;
  }

  private effectiveGain(): number {
    if (this.muted) {
      return 0;
    }
    // Slight perceptual curve so low slider positions stay smoothly audible.
    return this.volume * this.volume * 1.15;
  }

  private syncMasterGain(): void {
    if (this.ctx !== null && this.masterGain !== null) {
      this.masterGain.gain.setTargetAtTime(this.effectiveGain(), this.ctx.currentTime, 0.015);
    }
  }

  private playOne(ctx: AudioContext, dest: AudioNode, cue: SoundCue): void {
    const now = ctx.currentTime;
    switch (cue.kind) {
      case "cue_hit": {
        const k = Math.max(0.2, Math.min(1, cue.intensity));
        // Warm chalked leather tip thump + crisp maple shaft transient
        this.playTone(ctx, dest, now, "sine", 195, 78, 0.065, 0.3 * k);
        this.playTone(ctx, dest, now, "triangle", 460, 290, 0.032, 0.14 * k);
        this.playNoise(ctx, dest, now, 0.024, "bandpass", 1750, 2.2, 0.16 * k);
        break;
      }
      case "ball_click": {
        // Light rate-limit on dense rack breaks so 15-ball scatter stays crisp.
        const perfNow = performance.now();
        if (perfNow - this.lastClickMs < 15) {
          return;
        }
        this.lastClickMs = perfNow;
        const k = Math.max(0.15, Math.min(1, cue.intensity));
        // Subtle organic pitch jitter (±3%) so consecutive clicks sound like real phenolic balls.
        const jitter = 0.97 + Math.random() * 0.06;
        this.playTone(ctx, dest, now, "sine", 2820 * jitter, 2340 * jitter, 0.028, 0.19 * k);
        this.playTone(ctx, dest, now, "triangle", 4680 * jitter, 3920 * jitter, 0.016, 0.11 * k);
        this.playNoise(ctx, dest, now, 0.008, "highpass", 4200, 1.0, 0.12 * k);
        break;
      }
      case "cushion_thud": {
        const k = Math.max(0.15, Math.min(1, cue.intensity));
        // Muted vulcanized rubber rail thump + soft cloth brush
        this.playTone(ctx, dest, now, "sine", 128, 52, 0.07, 0.28 * k);
        this.playNoise(ctx, dest, now, 0.04, "lowpass", 420, 0.8, 0.12 * k);
        break;
      }
      case "pocket_drop": {
        // Hollow wooden/leather pocket chute drop + realistic secondary rattle bounce
        this.playTone(ctx, dest, now, "triangle", 205, 112, 0.07, 0.24);
        this.playNoise(ctx, dest, now + 0.01, 0.045, "bandpass", 650, 2.5, 0.12);
        this.playTone(ctx, dest, now + 0.06, "triangle", 168, 92, 0.065, 0.17);
        this.playTone(ctx, dest, now + 0.115, "sine", 240, 138, 0.05, 0.11);
        this.playNoise(ctx, dest, now + 0.11, 0.035, "bandpass", 920, 2.0, 0.08);
        break;
      }
      case "checker_move": {
        // Tactile hardwood checker set down on wooden board ("tock")
        const j = 0.97 + Math.random() * 0.06;
        this.playTone(ctx, dest, now, "sine", 275 * j, 135 * j, 0.042, 0.22);
        this.playTone(ctx, dest, now, "triangle", 760 * j, 520 * j, 0.022, 0.12);
        this.playNoise(ctx, dest, now, 0.014, "bandpass", 2150, 2.8, 0.14);
        break;
      }
      case "checker_capture": {
        // Crisp double hardwood clack (jump lift + solid capture landing)
        this.playTone(ctx, dest, now, "sine", 340, 185, 0.036, 0.18);
        this.playNoise(ctx, dest, now, 0.012, "bandpass", 2400, 2.6, 0.11);
        this.playTone(ctx, dest, now + 0.062, "sine", 310, 150, 0.048, 0.25);
        this.playTone(ctx, dest, now + 0.062, "triangle", 880, 610, 0.025, 0.14);
        this.playNoise(ctx, dest, now + 0.062, 0.016, "bandpass", 2600, 2.5, 0.16);
        break;
      }
      case "checker_king": {
        // Double-stacked wood clack + warm rising crowning triad
        this.playTone(ctx, dest, now, "sine", 290, 150, 0.04, 0.2);
        this.playNoise(ctx, dest, now, 0.014, "bandpass", 2200, 2.5, 0.13);
        this.playTone(ctx, dest, now + 0.04, "sine", 523.25, 523.25, 0.11, 0.16);
        this.playTone(ctx, dest, now + 0.11, "sine", 659.25, 659.25, 0.11, 0.16);
        this.playTone(ctx, dest, now + 0.18, "sine", 783.99, 783.99, 0.14, 0.18);
        this.playTone(ctx, dest, now + 0.25, "triangle", 1046.5, 1046.5, 0.2, 0.11);
        break;
      }
      case "clock_warn": {
        // Crisp mechanical tournament clock tick
        this.playTone(ctx, dest, now, "sine", 1040, 980, 0.032, 0.12);
        this.playNoise(ctx, dest, now, 0.008, "highpass", 3400, 1.2, 0.06);
        break;
      }
      case "game_over": {
        // Warm classic parlor victory/end fanfare
        this.playTone(ctx, dest, now, "sine", 392.0, 392.0, 0.2, 0.15);
        this.playTone(ctx, dest, now + 0.11, "sine", 493.88, 493.88, 0.22, 0.15);
        this.playTone(ctx, dest, now + 0.22, "sine", 587.33, 587.33, 0.26, 0.16);
        this.playTone(ctx, dest, now + 0.33, "triangle", 783.99, 783.99, 0.34, 0.13);
        break;
      }
    }
  }

  private playTone(
    ctx: AudioContext,
    dest: AudioNode,
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
    gain.gain.linearRampToValueAtTime(Math.max(0.001, peakGain), startTime + 0.003);
    gain.gain.exponentialRampToValueAtTime(0.0001, startTime + durationSec);
    osc.connect(gain);
    gain.connect(dest);
    osc.start(startTime);
    osc.stop(startTime + durationSec + 0.015);
  }

  private playNoise(
    ctx: AudioContext,
    dest: AudioNode,
    startTime: number,
    durationSec: number,
    filterType: BiquadFilterType,
    filterHz: number,
    q: number,
    peakGain: number,
  ): void {
    if (this.noiseBuffer === null) {
      return;
    }
    const src = ctx.createBufferSource();
    src.buffer = this.noiseBuffer;

    const filter = ctx.createBiquadFilter();
    filter.type = filterType;
    filter.frequency.setValueAtTime(filterHz, startTime);
    filter.Q.setValueAtTime(q, startTime);

    const gain = ctx.createGain();
    gain.gain.setValueAtTime(Math.max(0.001, peakGain), startTime);
    gain.gain.exponentialRampToValueAtTime(0.0001, startTime + durationSec);

    src.connect(filter);
    filter.connect(gain);
    gain.connect(dest);
    src.start(startTime);
    src.stop(startTime + durationSec + 0.01);
  }
}
