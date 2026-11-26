import type { BotLevelDto } from "../wasm_bridge.js";

interface PendingSearch {
  readonly id: number;
  readonly fen: string;
  readonly level: BotLevelDto;
  readonly startedAt: number;
  readonly onResult: (uci: string) => void;
  timeoutId: number | null;
}

const MIN_VISIBLE_THINK_MS = 260;

function searchConfig(level: BotLevelDto): {
  readonly skill: number;
  readonly depth: number;
  readonly movetime: number;
} {
  switch (level) {
    case "easy":
      return { skill: 2, depth: 4, movetime: 120 };
    case "medium":
      return { skill: 10, depth: 8, movetime: 320 };
    case "hard":
      return { skill: 20, depth: 14, movetime: 750 };
  }
}

/**
 * Browser-side single-player Chess AI powered by `nmrugg/stockfish.js`
 * (`stockfish-19-lite-single.js` + `.wasm`, GPL-3.0) inside a dedicated Web Worker.
 *
 * Uses the single-threaded lite WASM build so no COOP/COEP headers are required
 * (preserving Google Sign-In and static GitHub Pages compatibility).
 * Falls back cleanly to `onResult("")` if the worker fails to start so the
 * built-in Rust WASM fallback AI takes over seamlessly.
 */
export class StockfishEngine {
  private worker: Worker | null = null;
  private ready = false;
  private failed = false;
  private nextId = 1;
  private active: PendingSearch | null = null;
  private queued: PendingSearch | null = null;
  private appliedSkill: number | null = null;

  public requestMove(fen: string, level: BotLevelDto, onResult: (uci: string) => void): void {
    this.cancel();
    const job: PendingSearch = {
      id: this.nextId,
      fen,
      level,
      startedAt: performance.now(),
      onResult,
      timeoutId: null,
    };
    this.nextId += 1;

    if (this.failed) {
      this.deliverAfterMinDelay(job, "");
      return;
    }

    if (this.worker === null) {
      this.spawnWorker();
    }
    if (this.failed || this.worker === null) {
      this.deliverAfterMinDelay(job, "");
      return;
    }

    if (!this.ready) {
      this.queued = job;
      return;
    }

    this.startSearch(job);
  }

  public cancel(): void {
    this.nextId += 1;
    if (this.active !== null) {
      if (this.active.timeoutId !== null) {
        window.clearTimeout(this.active.timeoutId);
      }
      this.active = null;
      try {
        this.worker?.postMessage("stop");
      } catch {
        // Ignore postMessage errors on terminated workers.
      }
    }
    this.queued = null;
  }

  public dispose(): void {
    this.cancel();
    if (this.worker !== null) {
      this.worker.terminate();
      this.worker = null;
    }
    this.ready = false;
    this.appliedSkill = null;
  }

  private spawnWorker(): void {
    try {
      const workerUrl = new URL("./stockfish/stockfish-19-lite-single.js", window.location.href);
      const worker = new Worker(workerUrl);
      this.worker = worker;
      this.ready = false;
      this.appliedSkill = null;

      worker.onmessage = (ev: MessageEvent): void => {
        const line = typeof ev.data === "string" ? ev.data.trim() : "";
        if (line === "") {
          return;
        }
        this.handleLine(line);
      };

      worker.onerror = (): void => {
        this.handleWorkerFailure();
      };

      worker.postMessage("uci");
    } catch {
      this.handleWorkerFailure();
    }
  }

  private handleWorkerFailure(): void {
    this.failed = true;
    if (this.worker !== null) {
      this.worker.terminate();
      this.worker = null;
    }
    this.ready = false;
    const pending = this.active ?? this.queued;
    this.active = null;
    this.queued = null;
    if (pending !== null) {
      if (pending.timeoutId !== null) {
        window.clearTimeout(pending.timeoutId);
      }
      this.deliverAfterMinDelay(pending, "");
    }
  }

  private handleLine(line: string): void {
    if (line === "uciok") {
      this.worker?.postMessage("isready");
      return;
    }
    if (line === "readyok") {
      this.ready = true;
      if (this.queued !== null && this.active === null) {
        const next = this.queued;
        this.queued = null;
        this.startSearch(next);
      }
      return;
    }
    if (line.startsWith("bestmove ")) {
      const parts = line.split(/\s+/);
      const move = parts[1] ?? "";
      const job = this.active;
      if (job !== null) {
        if (job.timeoutId !== null) {
          window.clearTimeout(job.timeoutId);
        }
        this.active = null;
        const cleanUci = move === "(none)" || move === "0000" ? "" : move;
        this.deliverAfterMinDelay(job, cleanUci);
      }
    }
  }

  private startSearch(job: PendingSearch): void {
    if (this.worker === null) {
      this.deliverAfterMinDelay(job, "");
      return;
    }
    const cfg = searchConfig(job.level);
    this.active = job;

    // Watchdog timeout: if Stockfish stalls on a slow device, fall back cleanly to Rust AI.
    job.timeoutId = window.setTimeout(() => {
      if (this.active?.id === job.id) {
        this.active = null;
        try {
          this.worker?.postMessage("stop");
        } catch {
          // Ignore stop errors.
        }
        job.onResult("");
      }
    }, Math.max(4_000, cfg.movetime * 5));

    if (this.appliedSkill !== cfg.skill) {
      this.worker.postMessage(`setoption name Skill Level value ${cfg.skill}`);
      this.appliedSkill = cfg.skill;
    }
    this.worker.postMessage(`position fen ${job.fen}`);
    this.worker.postMessage(`go depth ${cfg.depth} movetime ${cfg.movetime}`);
  }

  private deliverAfterMinDelay(job: PendingSearch, uci: string): void {
    const elapsed = performance.now() - job.startedAt;
    const remain = Math.max(0, MIN_VISIBLE_THINK_MS - elapsed);
    const targetId = job.id;
    const emit = (): void => {
      // Only deliver if no newer search or cancellation superseded this job.
      if (this.nextId - 1 === targetId && this.active === null) {
        job.onResult(uci);
      }
    };
    if (remain <= 8) {
      emit();
    } else {
      window.setTimeout(emit, remain);
    }
  }
}
