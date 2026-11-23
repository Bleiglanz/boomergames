import "./styles/classic.css";

import { getStrings, type Strings } from "./i18n/strings.js";
import { StageRenderer } from "./render/renderer.js";
import {
  WasmBridge,
  type CheckersColorDto,
  type CheckersOutcomeDto,
  type CheckersSettingsDto,
  type CheckersView,
  type GameKindDto,
  type LocalModeDto,
  type UiInput,
  type ViewModel,
} from "./wasm_bridge.js";

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

function formatSeatName(
  seatIndex: 0 | 1,
  mode: LocalModeDto,
  s: Strings,
): string {
  if (seatIndex === 0) {
    return s.player1;
  }
  return mode.kind === "vs_bot" ? `${s.computer} (${mode.level})` : s.player2;
}

function formatColorName(color: CheckersColorDto, s: Strings): string {
  return color === "black" ? s.colorBlack : s.colorWhite;
}

function formatOutcome(
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

interface MountedShell {
  readonly game: GameKindDto;
  readonly stage: StageRenderer;
  readonly canvasHost: HTMLElement;
  readonly bannerHost: HTMLElement;
  readonly playersHost: HTMLElement;
  readonly sidePanelHost: HTMLElement;
}

async function bootstrap(): Promise<void> {
  const root = document.getElementById("app");
  if (root === null) {
    return;
  }

  const bridge = await WasmBridge.create();
  let mountedShell: MountedShell | null = null;
  let lastScreenKey = "";

  const dispatchAndRender = (input: UiInput): void => {
    if (bridge.dispatch(input)) {
      renderApp();
    }
  };

  const disposeShell = (): void => {
    if (mountedShell !== null) {
      mountedShell.stage.dispose();
      mountedShell = null;
    }
  };

  const renderApp = (): void => {
    const vm: ViewModel = bridge.viewModel();
    const s = getStrings(vm.lang);
    document.documentElement.lang = vm.lang;
    document.documentElement.dataset.contrast = vm.highContrast ? "high" : "standard";

    const screenKey =
      vm.screen.kind === "home"
        ? `home:${vm.lang}`
        : `shell:${vm.screen.game}:${vm.lang}`;

    // Fast path: stay mounted on the same GameShell so Three.js canvas keeps focus and drag state.
    if (
      vm.screen.kind === "game_shell" &&
      mountedShell !== null &&
      screenKey === lastScreenKey
    ) {
      updateGameShellInPlace(mountedShell, vm, s, dispatchAndRender);
      return;
    }

    lastScreenKey = screenKey;
    disposeShell();
    root.innerHTML = "";

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
    if (vm.screen.kind === "game_shell") {
      controls.append(
        makeButton(s.returnHome, () => dispatchAndRender({ kind: "return_home" })),
      );
    }
    controls.append(
      makeButton(s.highContrast, () => dispatchAndRender({ kind: "toggle_high_contrast" }), {
        active: vm.highContrast,
      }),
      makeButton(vm.lang === "en" ? "Deutsch" : "English", () =>
        dispatchAndRender({ kind: "set_language", lang: vm.lang === "en" ? "de" : "en" }),
      ),
    );
    header.append(brand, controls);
    root.append(header);

    // Main area
    const main = document.createElement("main");
    main.className = "main-area";

    if (vm.screen.kind === "home") {
      if (vm.lastError !== null) {
        const errBanner = document.createElement("div");
        errBanner.className = "error-banner";
        errBanner.setAttribute("role", "alert");
        errBanner.textContent = `Error: ${vm.lastError}`;
        main.append(errBanner);
      }
      main.append(renderHome(s, dispatchAndRender));
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
      boardCol.append(frame);

      const sidePanelHost = document.createElement("aside");
      sidePanelHost.className = "side-panel";

      shell.append(boardCol, sidePanelHost);
      main.append(shell);
      root.append(main);

      const stage = new StageRenderer(frame, {
        onSelectSquare: (square) =>
          dispatchAndRender({ kind: "checkers_select_square", square }),
        onDragMove: (from, to) =>
          dispatchAndRender({ kind: "checkers_drag_move", from, to }),
        onClearSelection: () => dispatchAndRender({ kind: "checkers_clear_selection" }),
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
      };
      updateGameShellInPlace(mountedShell, vm, s, dispatchAndRender);
    }

    if (main.parentElement !== root) {
      root.append(main);
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

  let lastTime = performance.now();
  const loop = (now: number): void => {
    const dt = now - lastTime;
    lastTime = now;

    if (bridge.tick(dt)) {
      renderApp();
    }

    if (mountedShell !== null) {
      mountedShell.stage.render(
        mountedShell.game === "pool" ? bridge.poolFrame(now) : undefined,
      );
    }
    window.requestAnimationFrame(loop);
  };
  window.requestAnimationFrame(loop);
}

function updateGameShellInPlace(
  shell: MountedShell,
  vm: ViewModel,
  s: Strings,
  dispatch: (input: UiInput) => void,
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
    renderCheckersHud(shell, view, vm.screen.mode, s, dispatch);
    return;
  }

  // Pool Milestone 0 shell fallback until Milestone 2
  shell.playersHost.innerHTML = "";
  shell.sidePanelHost.innerHTML = "";
  const heading = document.createElement("h2");
  heading.textContent = s.poolTitle;
  const modeBox = document.createElement("div");
  modeBox.className = "status-box";
  modeBox.textContent = `${s.modeLabel}: ${formatMode(vm.screen.mode, s)}`;
  const statusBox = document.createElement("div");
  statusBox.className = "status-box";
  statusBox.textContent = s.shellPoolReady;
  shell.sidePanelHost.append(
    heading,
    modeBox,
    statusBox,
    makeButton(s.returnHome, () => dispatch({ kind: "return_home" }), { primary: true }),
  );
}

function renderCheckersHud(
  shell: MountedShell,
  view: CheckersView,
  mode: LocalModeDto,
  s: Strings,
  dispatch: (input: UiInput) => void,
): void {
  const st = view.state;

  // Banners: outcome, draw offer, mandatory capture, bot thinking
  if (st.outcome !== null) {
    const banner = document.createElement("div");
    banner.className = "notice-banner";
    banner.setAttribute("role", "status");
    const msg = document.createElement("span");
    msg.textContent = formatOutcome(st.outcome, mode, s);
    banner.append(
      msg,
      makeButton(s.rematch, () => dispatch({ kind: "rematch" }), { primary: true }),
    );
    shell.bannerHost.append(banner);
  } else if (st.drawOfferedBy !== null) {
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

  // Players strip
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

  // Side panel
  shell.sidePanelHost.innerHTML = "";
  const heading = document.createElement("h2");
  heading.textContent = s.checkersTitle;

  const modeBox = document.createElement("div");
  modeBox.className = "status-box";
  modeBox.textContent = `${s.modeLabel}: ${formatMode(mode, s)}`;

  // Clock selector
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

  // Board display toggles
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

  // Match actions
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

  // Move list
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

  shell.sidePanelHost.append(
    heading,
    modeBox,
    clockGroup,
    displayRow,
    actionRow,
    movesHeading,
    moveList,
  );
}

function renderHome(
  s: Strings,
  dispatch: (input: UiInput) => void,
): HTMLElement {
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
