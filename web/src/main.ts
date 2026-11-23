import "./styles/classic.css";

import { getStrings, type Strings } from "./i18n/strings.js";
import { StageRenderer } from "./render/renderer.js";
import {
  WasmBridge,
  type GameKindDto,
  type LocalModeDto,
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

function makeButton(label: string, onClick: () => void, primary = false): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = primary ? "classic-btn primary" : "classic-btn";
  btn.textContent = label;
  btn.addEventListener("click", onClick);
  return btn;
}

async function bootstrap(): Promise<void> {
  const root = document.getElementById("app");
  if (root === null) {
    return;
  }

  const bridge = await WasmBridge.create();
  let stage: StageRenderer | null = null;
  let canvasHost: HTMLElement | null = null;

  const dispatchAndRender = (input: Parameters<WasmBridge["dispatch"]>[0]): void => {
    if (bridge.dispatch(input)) {
      renderApp();
    }
  };

  const renderApp = (): void => {
    const vm: ViewModel = bridge.viewModel();
    const s = getStrings(vm.lang);
    document.documentElement.lang = vm.lang;
    document.documentElement.dataset.contrast = vm.highContrast ? "high" : "standard";

    if (stage !== null) {
      stage.dispose();
      stage = null;
      canvasHost = null;
    }

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
      makeButton(s.highContrast, () => dispatchAndRender({ kind: "toggle_high_contrast" })),
      makeButton(vm.lang === "en" ? "Deutsch" : "English", () =>
        dispatchAndRender({ kind: "set_language", lang: vm.lang === "en" ? "de" : "en" }),
      ),
    );

    header.append(brand, controls);
    root.append(header);

    // Main content
    const main = document.createElement("main");
    main.className = "main-area";

    if (vm.lastError !== null) {
      const errBanner = document.createElement("div");
      errBanner.className = "error-banner";
      errBanner.setAttribute("role", "alert");
      errBanner.textContent = `Error: ${vm.lastError}`;
      main.append(errBanner);
    }

    if (vm.screen.kind === "home") {
      main.append(renderHome(s, dispatchAndRender));
    } else {
      const shell = document.createElement("div");
      shell.className = "shell-layout";

      const frame = document.createElement("div");
      frame.className = "canvas-frame";
      shell.append(frame);

      const panel = document.createElement("aside");
      panel.className = "side-panel";
      const heading = document.createElement("h2");
      heading.textContent = vm.screen.game === "checkers" ? s.checkersTitle : s.poolTitle;

      const modeBox = document.createElement("div");
      modeBox.className = "status-box";
      modeBox.textContent = `${s.modeLabel}: ${formatMode(vm.screen.mode, s)}`;

      const statusBox = document.createElement("div");
      statusBox.className = "status-box";
      statusBox.textContent =
        vm.screen.game === "checkers" ? s.shellCheckersReady : s.shellPoolReady;

      panel.append(
        heading,
        modeBox,
        statusBox,
        makeButton(s.returnHome, () => dispatchAndRender({ kind: "return_home" }), true),
      );
      shell.append(panel);
      main.append(shell);

      canvasHost = frame;
      stage = new StageRenderer(frame);
      stage.setGame(vm.screen.game);
      stage.resize(frame);
    }

    root.append(main);

    // Footer
    const footer = document.createElement("footer");
    footer.className = "footer-bar";
    footer.textContent = s.footerNote;
    root.append(footer);
  };

  renderApp();

  window.addEventListener("resize", () => {
    if (stage !== null && canvasHost !== null) {
      stage.resize(canvasHost);
    }
  });

  let lastTime = performance.now();
  const loop = (now: number): void => {
    const dt = now - lastTime;
    lastTime = now;

    if (bridge.tick(dt)) {
      renderApp();
    }

    if (stage !== null) {
      stage.render(bridge.poolFrame(now));
    }
    window.requestAnimationFrame(loop);
  };
  window.requestAnimationFrame(loop);
}

function renderHome(
  s: Strings,
  dispatch: (input: Parameters<WasmBridge["dispatch"]>[0]) => void,
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
          index === 0,
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
