import "./styles/classic.css";

import { SoundSynth } from "./audio/synth.js";
import { getStrings, type Strings } from "./i18n/strings.js";
import { OnlineSession, parseHashIntent } from "./online/session.js";
import { loadPrefs, savePrefs, type UserPrefs } from "./prefs.js";
import { StageRenderer } from "./render/renderer.js";
import { LobbyController } from "./ui/lobby.js";
import {
  WasmBridge,
  type AimAidLevelDto,
  type CheckersColorDto,
  type CheckersOutcomeDto,
  type CheckersSettingsDto,
  type CheckersView,
  type GameKindDto,
  type LocalModeDto,
  type PoolFoulDto,
  type PoolOutcomeDto,
  type PoolSettingsDto,
  type PoolView,
  type UiInput,
  type ViewModel,
} from "./wasm_bridge.js";

const BALL_CHIP_COLORS: readonly string[] = [
  "#ffffff",
  "#f2c91d",
  "#1e52d8",
  "#d62828",
  "#6e2c91",
  "#f27413",
  "#1a7f37",
  "#7b1e1e",
  "#161616",
  "#f2c91d",
  "#1e52d8",
  "#d62828",
  "#6e2c91",
  "#f27413",
  "#1a7f37",
  "#7b1e1e",
];

function formatMode(mode: LocalModeDto, s: Strings): string {
  switch (mode.kind) {
    case "hotseat":
      return s.hotseat;
    case "free_practice":
      return s.freePractice;
    case "vs_bot":
      switch (mode.level) {
        case "easy":
          return s.botEasy;
        case "medium":
          return s.botMedium;
        case "hard":
          return s.botHard;
      }
  }
}

function formatClock(ms: number): string {
  const totalSec = Math.ceil(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return `${String(min).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}

function formatSeatName(seatIndex: 0 | 1, mode: LocalModeDto, s: Strings): string {
  if (seatIndex === 0) {
    return s.player1;
  }
  return mode.kind === "vs_bot" ? `${s.computer} (${mode.level})` : s.player2;
}

function formatColorName(color: CheckersColorDto, s: Strings): string {
  return color === "black" ? s.colorBlack : s.colorWhite;
}

function formatCheckersOutcome(
  outcome: CheckersOutcomeDto,
  mode: LocalModeDto,
  s: Strings,
): string {
  if (outcome.kind === "win") {
    const winnerSeatIdx: 0 | 1 = outcome.winner_seat === "1" ? 0 : 1;
    const winnerLabel = `${formatSeatName(winnerSeatIdx, mode, s)} (${formatColorName(outcome.winner, s)})`;
    switch (outcome.reason) {
      case "no_moves":
        return `${winnerLabel} ${s.outcomeWinNoMoves}`;
      case "resign":
        return `${winnerLabel} ${s.outcomeWinResign}`;
      case "timeout":
        return `${winnerLabel} ${s.outcomeWinTimeout}`;
    }
  }
  switch (outcome.reason) {
    case "agreement":
      return s.outcomeDrawAgreement;
    case "threefold_repetition":
      return s.outcomeDrawRepetition;
    case "forty_move_rule":
      return s.outcomeDrawFortyMove;
  }
}

function formatPoolOutcome(outcome: PoolOutcomeDto, mode: LocalModeDto, s: Strings): string {
  const idx: 0 | 1 = outcome.winnerSeat === "1" ? 0 : 1;
  const winner = formatSeatName(idx, mode, s);
  switch (outcome.reason) {
    case "eight_ball_pocketed":
      return `${winner} ${s.poolWinEight}`;
    case "opponent_early_eight":
      return `${winner} ${s.poolWinEarlyEight}`;
    case "opponent_foul_on_eight":
      return `${winner} ${s.poolWinFoulEight}`;
    case "resign":
      return `${winner} ${s.poolWinResign}`;
  }
}

function formatPoolFoul(foul: PoolFoulDto, s: Strings): string {
  switch (foul) {
    case "scratch":
      return s.poolFoulScratch;
    case "wrong_first_ball":
      return s.poolFoulWrongBall;
    case "no_hit":
      return s.poolFoulNoHit;
    case "no_rail_after_contact":
      return s.poolFoulNoRail;
  }
}

function makeButton(
  label: string,
  onClick: () => void,
  opts?: { readonly primary?: boolean; readonly active?: boolean; readonly disabled?: boolean },
): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  const classes = ["classic-btn"];
  if (opts?.primary) {
    classes.push("primary");
  }
  if (opts?.active) {
    classes.push("active-toggle");
  }
  btn.className = classes.join(" ");
  btn.textContent = label;
  btn.disabled = opts?.disabled ?? false;
  btn.addEventListener("click", onClick);
  return btn;
}

function clockKeyFor(settings: CheckersSettingsDto): string {
  if (settings.clockMode === "unlimited") {
    return "unlimited";
  }
  if (settings.clockMode === "move") {
    return `move:${settings.moveTimerSecs}`;
  }
  return `game:${settings.gameMinutes}`;
}

function settingsForClockKey(key: string): CheckersSettingsDto {
  if (key === "unlimited") {
    return { clockMode: "unlimited", moveTimerSecs: 30, gameMinutes: 10 };
  }
  if (key === "move:30") {
    return { clockMode: "move", moveTimerSecs: 30, gameMinutes: 10 };
  }
  if (key === "game:5") {
    return { clockMode: "game", moveTimerSecs: 30, gameMinutes: 5 };
  }
  return { clockMode: "game", moveTimerSecs: 30, gameMinutes: 10 };
}

interface PoolSideControls {
  readonly modeBox: HTMLElement;
  readonly shootBtn: HTMLButtonElement;
  readonly aimLabel: HTMLLabelElement;
  readonly aimRange: HTMLInputElement;
  readonly aidSelect: HTMLSelectElement;
  readonly timerGroup: HTMLElement;
  readonly timerSelect: HTMLSelectElement;
  readonly callRow: HTMLLabelElement;
  readonly callBox: HTMLInputElement;
  readonly rerackBtn: HTMLButtonElement;
  readonly resignBtn: HTMLButtonElement;
  readonly rematchBtn: HTMLButtonElement;
  currentSettings: PoolSettingsDto;
}

interface PoolWidgets {
  readonly spinDot: HTMLElement;
  readonly powerMask: HTMLElement;
  readonly powerReadout: HTMLElement;
  readonly sideControls: PoolSideControls;
}

interface MountedShell {
  readonly game: GameKindDto;
  readonly stage: StageRenderer;
  readonly canvasHost: HTMLElement;
  readonly bannerHost: HTMLElement;
  readonly playersHost: HTMLElement;
  readonly sidePanelHost: HTMLElement;
  readonly poolWidgets: PoolWidgets | null;
}

async function bootstrap(): Promise<void> {
  const root = document.getElementById("app");
  if (root === null) {
    return;
  }

  const bridge = await WasmBridge.create();
  let prefs: UserPrefs = loadPrefs();
  const synth = new SoundSynth(prefs.muted);

  // Hydrate persisted zero-cookie localStorage preferences into WASM before first render.
  if (prefs.lang !== "en") {
    bridge.dispatch({ kind: "set_language", lang: prefs.lang });
  }
  if (prefs.highContrast) {
    bridge.dispatch({ kind: "toggle_high_contrast" });
  }
  if (prefs.checkersShowNumbers) {
    bridge.dispatch({ kind: "checkers_toggle_square_numbers" });
  }
  // Discard any bootstrap cues before first interaction.
  bridge.drainSoundCues();

  const onlineSession = new OnlineSession();
  const initialIntent = parseHashIntent();
  let onlineView =
    initialIntent.loginToken !== undefined || initialIntent.tableId !== undefined;
  let lobbyController: LobbyController | null = null;
  let mountedShell: MountedShell | null = null;
  let lastScreenKey = "";
  let liveRegion: HTMLElement | null = null;
  let contrastBtnRef: HTMLButtonElement | null = null;
  let lastAnnouncement = "";

  const openOnlineLounge = (intent?: { readonly loginToken?: string; readonly tableId?: string }): void => {
    onlineView = true;
    renderApp();
    void onlineSession.probeBackend().then((available) => {
      if (available) {
        onlineSession.connect(intent);
      }
    });
  };

  if (onlineView) {
    void onlineSession.probeBackend().then((available) => {
      if (available) {
        onlineSession.connect(initialIntent);
      }
    });
  }

  const persistFromVm = (vm: ViewModel): void => {
    const showNums =
      vm.screen.kind === "game_shell" && vm.screen.checkers !== null
        ? vm.screen.checkers.showSquareNumbers
        : prefs.checkersShowNumbers;
    prefs = {
      lang: vm.lang === "de" ? "de" : "en",
      highContrast: vm.highContrast,
      muted: synth.isMuted(),
      checkersShowNumbers: showNums,
    };
    savePrefs(prefs);
  };

  const announce = (text: string): void => {
    if (liveRegion !== null && text !== "" && text !== lastAnnouncement) {
      lastAnnouncement = text;
      liveRegion.textContent = text;
    }
  };

  const dispatchAndRender = (input: UiInput): void => {
    synth.unlock();
    const changed = bridge.dispatch(input);
    synth.playAll(bridge.drainSoundCues());
    if (changed) {
      renderApp();
      persistFromVm(bridge.viewModel());
    }
  };

  const disposeShell = (): void => {
    if (mountedShell !== null) {
      mountedShell.stage.dispose();
      mountedShell = null;
    }
    if (lobbyController !== null) {
      lobbyController.dispose();
      lobbyController = null;
    }
  };

  const renderApp = (): void => {
    const vm: ViewModel = bridge.viewModel();
    const s = getStrings(vm.lang);
    document.documentElement.lang = vm.lang;
    document.documentElement.dataset.contrast = vm.highContrast ? "high" : "standard";

    const screenKey = onlineView
      ? `lounge:${vm.lang}`
      : vm.screen.kind === "home"
        ? `home:${vm.lang}`
        : `shell:${vm.screen.game}:${vm.lang}`;

    if (
      !onlineView &&
      vm.screen.kind === "game_shell" &&
      mountedShell !== null &&
      screenKey === lastScreenKey
    ) {
      if (contrastBtnRef !== null) {
        contrastBtnRef.className = vm.highContrast
          ? "classic-btn active-toggle"
          : "classic-btn";
        contrastBtnRef.setAttribute("aria-pressed", String(vm.highContrast));
      }
      updateGameShellInPlace(mountedShell, vm, s, dispatchAndRender, announce);
      return;
    }

    lastScreenKey = screenKey;
    disposeShell();
    root.innerHTML = "";

    const skipLink = document.createElement("a");
    skipLink.className = "skip-link";
    skipLink.href = "#main-content";
    skipLink.textContent = s.skipToContent;
    root.append(skipLink);

    liveRegion = document.createElement("div");
    liveRegion.className = "sr-only";
    liveRegion.setAttribute("aria-live", "polite");
    liveRegion.setAttribute("aria-atomic", "true");
    root.append(liveRegion);

    // Header
    const header = document.createElement("header");
    header.className = "top-bar";

    const brand = document.createElement("div");
    brand.className = "brand";
    const h1 = document.createElement("h1");
    h1.className = "brand-title";
    h1.textContent = "Boomergames";
    const badge = document.createElement("span");
    badge.className = "brand-badge";
    badge.textContent = `${s.offlineBadge} v${vm.version}`;
    brand.append(h1, badge);

    const controls = document.createElement("div");
    controls.className = "top-controls";
    if (onlineView) {
      controls.append(
        makeButton(s.navOfflinePlay, () => {
          onlineView = false;
          renderApp();
        }),
      );
    } else {
      if (vm.screen.kind === "game_shell") {
        controls.append(
          makeButton(s.returnHome, () => dispatchAndRender({ kind: "return_home" })),
        );
      }
      controls.append(
        makeButton(s.navOnlineLounge, () => openOnlineLounge(), { primary: vm.screen.kind === "home" }),
      );
    }

    const soundBtn = makeButton(
      synth.isMuted() ? s.soundOff : s.soundOn,
      () => {
        synth.setMuted(!synth.isMuted());
        if (!synth.isMuted()) {
          synth.unlock();
        }
        soundBtn.textContent = synth.isMuted() ? s.soundOff : s.soundOn;
        soundBtn.className = synth.isMuted() ? "classic-btn" : "classic-btn active-toggle";
        soundBtn.setAttribute("aria-pressed", String(!synth.isMuted()));
        persistFromVm(bridge.viewModel());
      },
      { active: !synth.isMuted() },
    );
    soundBtn.setAttribute("aria-pressed", String(!synth.isMuted()));

    const contrastBtn = makeButton(
      s.highContrast,
      () => dispatchAndRender({ kind: "toggle_high_contrast" }),
      { active: vm.highContrast },
    );
    contrastBtn.setAttribute("aria-pressed", String(vm.highContrast));
    contrastBtnRef = contrastBtn;

    controls.append(
      soundBtn,
      contrastBtn,
      makeButton(vm.lang === "en" ? "Deutsch" : "English", () =>
        dispatchAndRender({ kind: "set_language", lang: vm.lang === "en" ? "de" : "en" }),
      ),
    );
    header.append(brand, controls);
    root.append(header);

    // Main area
    const main = document.createElement("main");
    main.id = "main-content";
    main.className = "main-area";

    if (onlineView) {
      const loungeHost = document.createElement("div");
      main.append(loungeHost);
      root.append(main);
      lobbyController = new LobbyController(loungeHost, onlineSession, s, () => {
        onlineView = false;
        renderApp();
      });
    } else if (vm.screen.kind === "home") {
      if (vm.lastError !== null) {
        const errBanner = document.createElement("div");
        errBanner.className = "error-banner";
        errBanner.setAttribute("role", "alert");
        errBanner.textContent = `Error: ${vm.lastError}`;
        main.append(errBanner);
      }
      main.append(renderHome(s, dispatchAndRender));
      root.append(main);
    } else {
      const bannerHost = document.createElement("div");
      main.append(bannerHost);

      const shell = document.createElement("div");
      shell.className = "shell-layout";

      const boardCol = document.createElement("div");
      boardCol.className = "board-column";

      const playersHost = document.createElement("div");
      boardCol.append(playersHost);

      const frame = document.createElement("div");
      frame.className =
        vm.screen.game === "checkers" ? "canvas-frame checkers-aspect" : "canvas-frame";

      const sidePanelHost = document.createElement("aside");
      sidePanelHost.className = "side-panel";

      let poolWidgets: PoolWidgets | null = null;
      if (vm.screen.game === "pool") {
        const landscapeNote = document.createElement("div");
        landscapeNote.className = "landscape-hint";
        landscapeNote.textContent = s.landscapeHint;
        boardCol.append(landscapeNote);

        const stageRow = document.createElement("div");
        stageRow.className = "pool-stage-row";

        const { box: spinBox, spinDot } = buildSpinWidget(s, dispatchAndRender);
        const { box: powerBox, powerMask, powerReadout } = buildPowerWidget(s, dispatchAndRender);
        const sideControls = buildPoolSideControls(sidePanelHost, s, dispatchAndRender);
        poolWidgets = { spinDot, powerMask, powerReadout, sideControls };

        stageRow.append(spinBox, frame, powerBox);
        boardCol.append(stageRow);
      } else {
        boardCol.append(frame);
      }

      shell.append(boardCol, sidePanelHost);
      main.append(shell);
      // Append before constructing StageRenderer so clientWidth/clientHeight are laid out.
      root.append(main);

      const stage = new StageRenderer(frame, {
        checkers: {
          onSelectSquare: (square) =>
            dispatchAndRender({ kind: "checkers_select_square", square }),
          onDragMove: (from, to) =>
            dispatchAndRender({ kind: "checkers_drag_move", from, to }),
          onClearSelection: () => dispatchAndRender({ kind: "checkers_clear_selection" }),
        },
        pool: {
          onAim: (angle) => dispatchAndRender({ kind: "pool_set_aim", angle }),
          onPower: (power) => dispatchAndRender({ kind: "pool_set_power", power }),
          onShoot: () => dispatchAndRender({ kind: "pool_shoot" }),
          onPreviewCue: (x, y) => dispatchAndRender({ kind: "pool_preview_cue", x, y }),
          onPlaceCue: (x, y) => dispatchAndRender({ kind: "pool_place_cue", x, y }),
          onCancelCuePreview: () => dispatchAndRender({ kind: "pool_cancel_cue_preview" }),
          onCallPocket: (pocket) => dispatchAndRender({ kind: "pool_call_pocket", pocket }),
        },
      });
      stage.setGame(vm.screen.game);
      stage.resize(frame);

      mountedShell = {
        game: vm.screen.game,
        stage,
        canvasHost: frame,
        bannerHost,
        playersHost,
        sidePanelHost,
        poolWidgets,
      };
      updateGameShellInPlace(mountedShell, vm, s, dispatchAndRender, announce);
    }

    // Footer
    const footer = document.createElement("footer");
    footer.className = "footer-bar";
    footer.textContent = s.footerNote;
    root.append(footer);
  };

  renderApp();

  window.addEventListener("resize", () => {
    if (mountedShell !== null) {
      mountedShell.stage.resize(mountedShell.canvasHost);
    }
  });

  // Pool keyboard controls: Left/Right aim (Shift = fine), Up/Down power, Space shoot
  window.addEventListener("keydown", (ev) => {
    if (mountedShell?.game !== "pool") {
      return;
    }
    const target = ev.target;
    const isNativeControl =
      target instanceof HTMLButtonElement ||
      target instanceof HTMLSelectElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLInputElement && target.type !== "range");

    if (ev.code === "Space" || ev.key === " ") {
      if (!ev.repeat && !isNativeControl) {
        ev.preventDefault();
        dispatchAndRender({ kind: "pool_shoot" });
      }
      return;
    }

    if (isNativeControl || target instanceof HTMLInputElement) {
      return;
    }

    const vm = bridge.viewModel();
    if (vm.screen.kind !== "game_shell" || vm.screen.pool === null) {
      return;
    }
    const pool = vm.screen.pool;
    if (pool.animating || pool.state.outcome !== null || pool.state.botThinking) {
      return;
    }

    if (ev.key === "ArrowLeft" || ev.key === "ArrowRight") {
      ev.preventDefault();
      const stepDeg = ev.shiftKey ? 0.25 : 1.5;
      const delta = ((ev.key === "ArrowLeft" ? 1 : -1) * stepDeg * Math.PI) / 180;
      dispatchAndRender({ kind: "pool_set_aim", angle: pool.aimAngle + delta });
    } else if (ev.key === "ArrowUp" || ev.key === "ArrowDown") {
      ev.preventDefault();
      const step = ev.shiftKey ? 0.02 : 0.05;
      const nextPower = pool.power + (ev.key === "ArrowUp" ? step : -step);
      dispatchAndRender({ kind: "pool_set_power", power: nextPower });
    }
  });

  let lastTime = performance.now();
  const loop = (now: number): void => {
    const dt = now - lastTime;
    lastTime = now;

    if (bridge.tick(dt)) {
      renderApp();
    }
    synth.playAll(bridge.drainSoundCues());

    if (mountedShell !== null) {
      mountedShell.stage.render(
        mountedShell.game === "pool" ? bridge.poolFrame(now) : undefined,
      );
    }
    window.requestAnimationFrame(loop);
  };
  window.requestAnimationFrame(loop);
}

function buildSpinWidget(
  s: Strings,
  dispatch: (input: UiInput) => void,
): { readonly box: HTMLElement; readonly spinDot: HTMLElement } {
  const box = document.createElement("div");
  box.className = "pool-widget-box";

  const label = document.createElement("div");
  label.className = "pool-widget-label";
  label.textContent = s.poolSpinLabel;

  const pad = document.createElement("div");
  pad.className = "spin-pad";
  pad.setAttribute("role", "slider");
  pad.setAttribute("aria-label", s.poolSpinLabel);

  const chH = document.createElement("div");
  chH.className = "spin-crosshair-h";
  const chV = document.createElement("div");
  chV.className = "spin-crosshair-v";
  const spinDot = document.createElement("div");
  spinDot.className = "spin-dot";
  spinDot.style.left = "50%";
  spinDot.style.top = "50%";
  pad.append(chH, chV, spinDot);

  let dragging = false;
  const updateFromPointer = (ev: PointerEvent): void => {
    const rect = pad.getBoundingClientRect();
    const nx = ((ev.clientX - rect.left) / rect.width) * 2 - 1;
    const ny = -(((ev.clientY - rect.top) / rect.height) * 2 - 1);
    dispatch({ kind: "pool_set_spin", spin_x: nx, spin_y: ny });
  };
  pad.addEventListener("pointerdown", (ev) => {
    dragging = true;
    pad.setPointerCapture(ev.pointerId);
    updateFromPointer(ev);
  });
  pad.addEventListener("pointermove", (ev) => {
    if (dragging) {
      updateFromPointer(ev);
    }
  });
  pad.addEventListener("pointerup", (ev) => {
    dragging = false;
    if (pad.hasPointerCapture(ev.pointerId)) {
      pad.releasePointerCapture(ev.pointerId);
    }
  });

  const resetBtn = makeButton(s.poolSpinCenter, () =>
    dispatch({ kind: "pool_set_spin", spin_x: 0, spin_y: 0 }),
  );
  resetBtn.style.minHeight = "2rem";
  resetBtn.style.fontSize = "0.8rem";
  resetBtn.style.padding = "0.2rem 0.45rem";

  box.append(label, pad, resetBtn);
  return { box, spinDot };
}

function buildPowerWidget(
  s: Strings,
  dispatch: (input: UiInput) => void,
): {
  readonly box: HTMLElement;
  readonly powerMask: HTMLElement;
  readonly powerReadout: HTMLElement;
} {
  const box = document.createElement("div");
  box.className = "pool-widget-box";

  const label = document.createElement("div");
  label.className = "pool-widget-label";
  label.textContent = s.poolPowerLabel;

  const track = document.createElement("div");
  track.className = "power-track";
  track.setAttribute("role", "slider");
  track.setAttribute("aria-label", s.poolPowerLabel);

  const powerMask = document.createElement("div");
  powerMask.className = "power-fill-mask";
  const ticks = document.createElement("div");
  ticks.className = "power-tick-marks";
  track.append(powerMask, ticks);

  const powerReadout = document.createElement("div");
  powerReadout.className = "pool-widget-label";
  powerReadout.textContent = "65%";

  let pulling = false;
  const setPowerFromPointer = (ev: PointerEvent): void => {
    const rect = track.getBoundingClientRect();
    // Classic Yahoo! Pool pull-down meter: pulling further down increases shot power.
    const ratio = Math.max(0.04, Math.min(1.0, (ev.clientY - rect.top) / rect.height));
    dispatch({ kind: "pool_set_power", power: ratio });
  };
  track.addEventListener("pointerdown", (ev) => {
    pulling = true;
    track.setPointerCapture(ev.pointerId);
    setPowerFromPointer(ev);
  });
  track.addEventListener("pointermove", (ev) => {
    if (pulling) {
      setPowerFromPointer(ev);
    }
  });
  track.addEventListener("pointerup", (ev) => {
    if (!pulling) {
      return;
    }
    pulling = false;
    if (track.hasPointerCapture(ev.pointerId)) {
      track.releasePointerCapture(ev.pointerId);
    }
    setPowerFromPointer(ev);
    dispatch({ kind: "pool_shoot" });
  });

  box.append(label, track, powerReadout);
  return { box, powerMask, powerReadout };
}

function buildPoolSideControls(
  host: HTMLElement,
  s: Strings,
  dispatch: (input: UiInput) => void,
): PoolSideControls {
  host.innerHTML = "";
  const heading = document.createElement("h2");
  heading.textContent = s.poolTitle;

  const modeBox = document.createElement("div");
  modeBox.className = "status-box";

  const shootBtn = makeButton(s.poolShootBtn, () => dispatch({ kind: "pool_shoot" }), {
    primary: true,
  });

  // Persistent fine-aim slider (0.1 deg steps) so dragging is never interrupted by DOM re-renders.
  const aimGroup = document.createElement("div");
  aimGroup.className = "control-group";
  const aimLabel = document.createElement("label");
  aimLabel.htmlFor = "pool-fine-aim";
  aimLabel.textContent = `${s.poolFineAimLabel}: 0.0°`;
  const aimRange = document.createElement("input");
  aimRange.id = "pool-fine-aim";
  aimRange.type = "range";
  aimRange.className = "range-input";
  aimRange.min = "-180";
  aimRange.max = "180";
  aimRange.step = "0.1";
  aimRange.value = "0";
  aimRange.addEventListener("input", () => {
    const deg = Number(aimRange.value);
    aimLabel.textContent = `${s.poolFineAimLabel}: ${deg.toFixed(1)}°`;
    dispatch({ kind: "pool_set_aim", angle: (deg * Math.PI) / 180 });
  });
  aimGroup.append(aimLabel, aimRange);

  const controls: PoolSideControls = {
    modeBox,
    shootBtn,
    aimLabel,
    aimRange,
    aidSelect: document.createElement("select"),
    timerGroup: document.createElement("div"),
    timerSelect: document.createElement("select"),
    callRow: document.createElement("label"),
    callBox: document.createElement("input"),
    rerackBtn: makeButton(s.poolRerackBtn, () => dispatch({ kind: "pool_rerack" })),
    resignBtn: makeButton(s.resign, () => dispatch({ kind: "resign" })),
    rematchBtn: makeButton(s.rematch, () => dispatch({ kind: "rematch" })),
    currentSettings: {
      aimAid: "extended",
      callEight: false,
      ballInHand: "anywhere",
      eightOnBreak: "re_rack",
      shotTimerSecs: 30,
    },
  };

  // Aiming aid selector
  const aidGroup = document.createElement("div");
  aidGroup.className = "control-group";
  const aidLabel = document.createElement("label");
  aidLabel.htmlFor = "pool-aim-aid";
  aidLabel.textContent = s.poolAimAidLabel;
  controls.aidSelect.id = "pool-aim-aid";
  controls.aidSelect.className = "classic-select";
  const aidOpts: readonly [AimAidLevelDto, string][] = [
    ["extended", s.poolAimExtended],
    ["basic", s.poolAimBasic],
    ["off", s.poolAimOff],
  ];
  for (const [val, text] of aidOpts) {
    const opt = document.createElement("option");
    opt.value = val;
    opt.textContent = text;
    controls.aidSelect.append(opt);
  }
  controls.aidSelect.addEventListener("change", () => {
    controls.aidSelect.blur();
    dispatch({
      kind: "pool_set_settings",
      settings: {
        ...controls.currentSettings,
        aimAid: controls.aidSelect.value as AimAidLevelDto,
      },
    });
  });
  aidGroup.append(aidLabel, controls.aidSelect);

  // Shot timer selector
  controls.timerGroup.className = "control-group";
  const tLabel = document.createElement("label");
  tLabel.htmlFor = "pool-timer";
  tLabel.textContent = s.poolTimerLabel;
  controls.timerSelect.id = "pool-timer";
  controls.timerSelect.className = "classic-select";
  for (const secs of [0, 20, 30, 60]) {
    const opt = document.createElement("option");
    opt.value = String(secs);
    opt.textContent = secs === 0 ? s.clockUnlimited : `${secs} s`;
    controls.timerSelect.append(opt);
  }
  controls.timerSelect.addEventListener("change", () => {
    controls.timerSelect.blur();
    dispatch({
      kind: "pool_set_settings",
      settings: {
        ...controls.currentSettings,
        shotTimerSecs: Number(controls.timerSelect.value),
      },
    });
  });
  controls.timerGroup.append(tLabel, controls.timerSelect);

  // Call 8-ball checkbox
  controls.callRow.className = "checkbox-row";
  controls.callBox.type = "checkbox";
  controls.callBox.addEventListener("change", () => {
    dispatch({
      kind: "pool_set_settings",
      settings: {
        ...controls.currentSettings,
        callEight: controls.callBox.checked,
      },
    });
  });
  controls.callRow.append(controls.callBox, document.createTextNode(s.poolCallEightLabel));

  const actions = document.createElement("div");
  actions.className = "control-row";
  actions.append(controls.rerackBtn, controls.resignBtn, controls.rematchBtn);

  const keysHint = document.createElement("p");
  keysHint.className = "keys-hint";
  keysHint.textContent = s.poolKeysHint;

  host.append(
    heading,
    modeBox,
    shootBtn,
    aimGroup,
    aidGroup,
    controls.timerGroup,
    controls.callRow,
    actions,
    keysHint,
  );
  return controls;
}

function updateGameShellInPlace(
  shell: MountedShell,
  vm: ViewModel,
  s: Strings,
  dispatch: (input: UiInput) => void,
  announce: (text: string) => void,
): void {
  if (vm.screen.kind !== "game_shell") {
    return;
  }

  shell.bannerHost.innerHTML = "";
  if (vm.lastError !== null) {
    const err = document.createElement("div");
    err.className = "error-banner";
    err.setAttribute("role", "alert");
    err.textContent =
      vm.lastError === "not_allowed" && vm.screen.game === "checkers"
        ? s.drawDeclinedError
        : `Error: ${vm.lastError}`;
    shell.bannerHost.append(err);
  }

  if (vm.screen.game === "checkers" && vm.screen.checkers !== null) {
    const view = vm.screen.checkers;
    shell.stage.syncCheckers(view);
    renderCheckersHud(shell, view, vm.screen.mode, s, dispatch, announce);
    return;
  }

  if (vm.screen.game === "pool" && vm.screen.pool !== null) {
    const view = vm.screen.pool;
    shell.stage.syncPool(view);
    renderPoolHud(shell, view, vm.screen.mode, s, dispatch, announce);
  }
}

function renderPoolHud(
  shell: MountedShell,
  view: PoolView,
  mode: LocalModeDto,
  s: Strings,
  dispatch: (input: UiInput) => void,
  announce: (text: string) => void,
): void {
  const st = view.state;

  // Update spin dot & vertical power bar smoothly without recreating DOM nodes
  if (shell.poolWidgets !== null) {
    const px = 50 + view.spinX * 38;
    const py = 50 - view.spinY * 38;
    shell.poolWidgets.spinDot.style.left = `${px}%`;
    shell.poolWidgets.spinDot.style.top = `${py}%`;

    const pct = Math.round(view.power * 100);
    shell.poolWidgets.powerMask.style.height = `${100 - pct}%`;
    shell.poolWidgets.powerReadout.textContent = `${pct}%`;
  }

  // Status / Outcome / Foul / Ball-in-hand banners
  if (st.outcome !== null) {
    const outcomeText = formatPoolOutcome(st.outcome, mode, s);
    announce(outcomeText);
    const banner = document.createElement("div");
    banner.className = "notice-banner";
    banner.setAttribute("role", "status");
    const msg = document.createElement("span");
    msg.textContent = outcomeText;
    banner.append(
      msg,
      makeButton(s.rematch, () => dispatch({ kind: "rematch" }), { primary: true }),
    );
    shell.bannerHost.append(banner);
  } else if (st.botThinking) {
    const banner = document.createElement("div");
    banner.className = "notice-banner";
    banner.textContent = s.botThinking;
    shell.bannerHost.append(banner);
  } else if (!view.animating) {
    if (st.reracked) {
      announce(s.poolRerackedNotice);
      const b = document.createElement("div");
      b.className = "notice-banner";
      b.textContent = s.poolRerackedNotice;
      shell.bannerHost.append(b);
    } else if (st.lastFouls.length > 0) {
      const firstFoul = st.lastFouls[0];
      if (firstFoul !== undefined) {
        const foulText = formatPoolFoul(firstFoul, s);
        announce(foulText);
        const b = document.createElement("div");
        b.className = "error-banner";
        b.textContent = foulText;
        shell.bannerHost.append(b);
      }
    }

    if (st.isBreakShot) {
      const b = document.createElement("div");
      b.className = "notice-banner";
      b.textContent = s.poolBreakHint;
      shell.bannerHost.append(b);
    } else if (st.ballInHand) {
      const b = document.createElement("div");
      b.className = "notice-banner";
      b.textContent = st.ballInHandBehindLine ? s.poolBallInHandBehind : s.poolBallInHand;
      shell.bannerHost.append(b);
    } else if (st.settings.callEight && st.isOnEightBall) {
      const b = document.createElement("div");
      b.className = "notice-banner";
      b.textContent = s.poolCallPocketHint;
      shell.bannerHost.append(b);
    }
  }

  // Players strip + pocketed balls + shot clock
  shell.playersHost.innerHTML = "";
  const strip = document.createElement("div");
  strip.className = "players-strip";
  const seats: readonly (0 | 1)[] = st.freePractice ? [0] : [0, 1];
  for (const idx of seats) {
    const seatDto = idx === 0 ? "1" : "2";
    const isActive = st.outcome === null && st.activeSeat === seatDto;
    const group = st.groups[idx];
    const pocketed = st.pocketedBySeat[idx];

    const card = document.createElement("div");
    card.className = isActive ? "player-card active" : "player-card";

    const left = document.createElement("div");
    const nameEl = document.createElement("div");
    nameEl.className = "player-name";
    const groupText =
      st.freePractice
        ? s.freePractice
        : isActive && st.isOnEightBall
          ? s.poolShootEight
          : group === "solids"
            ? s.poolSolids
            : group === "stripes"
              ? s.poolStripes
              : s.poolOpenTable;
    nameEl.textContent = `${isActive ? "▶ " : ""}${formatSeatName(idx, mode, s)} · ${groupText}`;

    const tray = document.createElement("div");
    tray.className = "pocketed-tray";
    for (const ballId of pocketed) {
      const chip = document.createElement("span");
      chip.className = ballId >= 9 ? "ball-chip stripe" : "ball-chip";
      chip.textContent = String(ballId);
      chip.style.backgroundColor = BALL_CHIP_COLORS[ballId] ?? "#ffffff";
      chip.style.color = ballId === 1 || ballId === 9 ? "#111111" : "#ffffff";
      tray.append(chip);
    }
    left.append(nameEl, tray);
    card.append(left);

    if (isActive && !st.freePractice && st.settings.shotTimerSecs > 0) {
      const clock = document.createElement("div");
      const urgent = st.turnMsLeft <= 5000;
      clock.className = urgent ? "clock-readout urgent" : "clock-readout";
      clock.textContent = formatClock(st.turnMsLeft);
      card.append(clock);
    }

    strip.append(card);
  }
  shell.playersHost.append(strip);

  // Update persistent Pool side-panel controls in place (never tears down slider mid-drag)
  if (shell.poolWidgets !== null) {
    const c = shell.poolWidgets.sideControls;
    c.currentSettings = st.settings;
    c.modeBox.textContent = `${s.modeLabel}: ${formatMode(mode, s)}`;

    const canShoot = !view.animating && st.outcome === null && !st.botThinking;
    c.shootBtn.disabled = !canShoot;
    c.aimRange.disabled = !canShoot;

    // Normalize aim angle cleanly into [-180, 180] degrees
    let deg = ((view.aimAngle * 180) / Math.PI) % 360;
    if (deg > 180) {
      deg -= 360;
    } else if (deg < -180) {
      deg += 360;
    }
    c.aimLabel.textContent = `${s.poolFineAimLabel}: ${deg.toFixed(1)}°`;
    if (document.activeElement !== c.aimRange) {
      c.aimRange.value = deg.toFixed(1);
    }

    c.aidSelect.value = st.settings.aimAid;
    c.timerSelect.value = String(st.settings.shotTimerSecs);
    c.callBox.checked = st.settings.callEight;

    c.timerGroup.style.display = st.freePractice ? "none" : "flex";
    c.callRow.style.display = st.freePractice ? "none" : "flex";
    c.rerackBtn.style.display = st.freePractice ? "inline-block" : "none";
    c.rerackBtn.disabled = view.animating;
    c.resignBtn.style.display = st.freePractice ? "none" : "inline-block";
    c.resignBtn.disabled = st.outcome !== null;
    c.rematchBtn.style.display = st.freePractice ? "none" : "inline-block";
    c.rematchBtn.className = st.outcome !== null ? "classic-btn primary" : "classic-btn";
  }
}

function renderCheckersHud(
  shell: MountedShell,
  view: CheckersView,
  mode: LocalModeDto,
  s: Strings,
  dispatch: (input: UiInput) => void,
  announce: (text: string) => void,
): void {
  const st = view.state;

  if (st.outcome !== null) {
    const outcomeText = formatCheckersOutcome(st.outcome, mode, s);
    announce(outcomeText);
    const banner = document.createElement("div");
    banner.className = "notice-banner";
    banner.setAttribute("role", "status");
    const msg = document.createElement("span");
    msg.textContent = outcomeText;
    banner.append(
      msg,
      makeButton(s.rematch, () => dispatch({ kind: "rematch" }), { primary: true }),
    );
    shell.bannerHost.append(banner);
  } else if (st.drawOfferedBy !== null) {
    announce(s.drawOfferedPrompt);
    const banner = document.createElement("div");
    banner.className = "notice-banner";
    banner.setAttribute("role", "status");
    const msg = document.createElement("span");
    msg.textContent = s.drawOfferedPrompt;
    const actions = document.createElement("div");
    actions.className = "control-row";
    actions.append(
      makeButton(s.acceptDraw, () => dispatch({ kind: "answer_draw", accept: true }), {
        primary: true,
      }),
      makeButton(s.declineDraw, () => dispatch({ kind: "answer_draw", accept: false })),
    );
    banner.append(msg, actions);
    shell.bannerHost.append(banner);
  } else if (st.mandatoryCapture) {
    announce(s.mustCapture);
    const banner = document.createElement("div");
    banner.className = "notice-banner";
    banner.setAttribute("role", "status");
    banner.textContent = s.mustCapture;
    shell.bannerHost.append(banner);
  } else if (st.botThinking) {
    const banner = document.createElement("div");
    banner.className = "notice-banner";
    banner.setAttribute("role", "status");
    banner.textContent = s.botThinking;
    shell.bannerHost.append(banner);
  }

  shell.playersHost.innerHTML = "";
  const strip = document.createElement("div");
  strip.className = "players-strip";
  const seats: readonly [0, 1] = [0, 1];
  for (const idx of seats) {
    const seatDto = idx === 0 ? "1" : "2";
    const color = st.seatColors[idx];
    const isActive = st.outcome === null && st.activeSeat === seatDto;
    const captured = color === "black" ? st.capturedByBlack : st.capturedByWhite;

    const card = document.createElement("div");
    card.className = isActive ? "player-card active" : "player-card";

    const meta = document.createElement("div");
    meta.className = "player-meta";

    const disc = document.createElement("span");
    disc.className = `disc-badge ${color}`;
    disc.setAttribute("aria-hidden", "true");

    const textWrap = document.createElement("div");
    const nameEl = document.createElement("div");
    nameEl.className = "player-name";
    nameEl.textContent = `${isActive ? "▶ " : ""}${formatSeatName(idx, mode, s)} (${formatColorName(color, s)})`;
    const subEl = document.createElement("div");
    subEl.className = "player-sub";
    subEl.textContent = `${s.captured}: ${captured}`;
    textWrap.append(nameEl, subEl);
    meta.append(disc, textWrap);
    card.append(meta);

    if (st.settings.clockMode !== "unlimited") {
      const clock = document.createElement("div");
      clock.className = "clock-readout";
      const ms =
        st.settings.clockMode === "game"
          ? st.clocksMs[idx]
          : isActive
            ? st.turnMsLeft
            : st.settings.moveTimerSecs * 1000;
      clock.textContent = formatClock(ms);
      card.append(clock);
    }

    strip.append(card);
  }
  shell.playersHost.append(strip);

  shell.sidePanelHost.innerHTML = "";
  const heading = document.createElement("h2");
  heading.textContent = s.checkersTitle;

  const modeBox = document.createElement("div");
  modeBox.className = "status-box";
  modeBox.textContent = `${s.modeLabel}: ${formatMode(mode, s)}`;

  const clockGroup = document.createElement("div");
  clockGroup.className = "control-group";
  const clockLabel = document.createElement("label");
  clockLabel.htmlFor = "checkers-clock-select";
  clockLabel.textContent = s.clockPreset;
  const select = document.createElement("select");
  select.id = "checkers-clock-select";
  select.className = "classic-select";
  const options: readonly [string, string][] = [
    ["game:10", s.clockGame10],
    ["game:5", s.clockGame5],
    ["move:30", s.clockMove30],
    ["unlimited", s.clockUnlimited],
  ];
  const currentKey = clockKeyFor(st.settings);
  for (const [val, text] of options) {
    const opt = document.createElement("option");
    opt.value = val;
    opt.textContent = text;
    opt.selected = val === currentKey;
    select.append(opt);
  }
  select.addEventListener("change", () => {
    dispatch({ kind: "checkers_set_clock", settings: settingsForClockKey(select.value) });
  });
  clockGroup.append(clockLabel, select);

  const displayRow = document.createElement("div");
  displayRow.className = "control-row";
  displayRow.append(
    makeButton(s.flipBoard, () => dispatch({ kind: "checkers_flip_board" }), {
      active: view.flipped,
    }),
    makeButton(s.showNumbers, () => dispatch({ kind: "checkers_toggle_square_numbers" }), {
      active: view.showSquareNumbers,
    }),
  );

  const actionRow = document.createElement("div");
  actionRow.className = "control-row";
  const inProgress = st.outcome === null;
  actionRow.append(
    makeButton(s.offerDraw, () => dispatch({ kind: "offer_draw" }), {
      disabled: !inProgress || st.drawOfferedBy !== null,
    }),
    makeButton(s.resign, () => dispatch({ kind: "resign" }), {
      disabled: !inProgress,
    }),
    makeButton(s.rematch, () => dispatch({ kind: "rematch" }), {
      primary: !inProgress,
    }),
  );

  const movesHeading = document.createElement("h3");
  movesHeading.textContent = s.moveListTitle;
  const moveList = document.createElement("div");
  moveList.className = "move-list";
  if (st.moves.length === 0) {
    moveList.textContent = s.noMovesYet;
  } else {
    for (let i = 0; i < st.moves.length; i += 2) {
      const row = document.createElement("div");
      row.className = "move-row";
      const num = document.createElement("span");
      num.textContent = `${Math.floor(i / 2) + 1}.`;
      const blackMv = document.createElement("span");
      blackMv.textContent = st.moves[i] ?? "";
      const whiteMv = document.createElement("span");
      whiteMv.textContent = st.moves[i + 1] ?? "";
      row.append(num, blackMv, whiteMv);
      moveList.append(row);
    }
    moveList.scrollTop = moveList.scrollHeight;
  }

  const keysHint = document.createElement("p");
  keysHint.className = "keys-hint";
  keysHint.textContent = s.checkersKeysHint;

  shell.sidePanelHost.append(
    heading,
    modeBox,
    clockGroup,
    displayRow,
    actionRow,
    movesHeading,
    moveList,
    keysHint,
  );
}

function renderHome(s: Strings, dispatch: (input: UiInput) => void): HTMLElement {
  const grid = document.createElement("div");
  grid.className = "home-grid";

  const addCard = (
    title: string,
    desc: string,
    game: GameKindDto,
    modes: readonly LocalModeDto[],
  ): void => {
    const card = document.createElement("section");
    card.className = "game-card";
    const h2 = document.createElement("h2");
    h2.textContent = title;
    const p = document.createElement("p");
    p.textContent = desc;

    const btnRow = document.createElement("div");
    btnRow.className = "mode-buttons";
    modes.forEach((mode, index) => {
      btnRow.append(
        makeButton(
          formatMode(mode, s),
          () => dispatch({ kind: "start_local_game", game, mode }),
          { primary: index === 0 },
        ),
      );
    });

    card.append(h2, p, btnRow);
    grid.append(card);
  };

  addCard(s.checkersTitle, s.checkersDesc, "checkers", [
    { kind: "hotseat" },
    { kind: "vs_bot", level: "easy" },
    { kind: "vs_bot", level: "medium" },
    { kind: "vs_bot", level: "hard" },
  ]);

  addCard(s.poolTitle, s.poolDesc, "pool", [
    { kind: "hotseat" },
    { kind: "vs_bot", level: "easy" },
    { kind: "vs_bot", level: "medium" },
    { kind: "vs_bot", level: "hard" },
    { kind: "free_practice" },
  ]);

  return grid;
}

void bootstrap();
