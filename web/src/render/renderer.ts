import * as THREE from "three";

import { CheckersScene, type CheckersBoardCallbacks } from "./checkers_scene.js";
import {
  POOL_RAIL_MARGIN,
  POOL_STAGE_BORDER,
  POOL_TABLE_HEIGHT,
  POOL_TABLE_WIDTH,
  PoolScene,
  type PoolTableCallbacks,
} from "./pool_scene.js";
import type { CheckersView, GameKindDto, PoolView } from "../wasm_bridge.js";

export interface StageCallbacks {
  readonly checkers: CheckersBoardCallbacks;
  readonly pool: PoolTableCallbacks;
}

export class StageRenderer {
  private readonly renderer: THREE.WebGLRenderer;
  private readonly camera: THREE.OrthographicCamera;
  private readonly scene: THREE.Scene;
  private readonly poolScene: PoolScene;
  private readonly checkersScene: CheckersScene;
  private activeGame: GameKindDto | null = null;
  private readonly canvas: HTMLCanvasElement;
  private readonly container: HTMLElement;
  private readonly resizeObserver: ResizeObserver;
  private lastAspect = 1;

  private readonly onPointerDownBound: (ev: PointerEvent) => void;
  private readonly onPointerMoveBound: (ev: PointerEvent) => void;
  private readonly onPointerUpBound: (ev: PointerEvent) => void;
  private readonly onKeyDownBound: (ev: KeyboardEvent) => void;

  public constructor(container: HTMLElement, callbacks: StageCallbacks) {
    this.container = container;
    this.renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: "default" });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.canvas = this.renderer.domElement;
    this.canvas.tabIndex = 0;
    this.canvas.setAttribute("aria-label", "Interactive game board");
    container.innerHTML = "";
    container.appendChild(this.canvas);

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x1f3a24);
    this.camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 20);
    this.camera.position.set(0, 0, 5);
    this.camera.lookAt(0, 0, 0);

    this.poolScene = new PoolScene(callbacks.pool);
    this.checkersScene = new CheckersScene(callbacks.checkers);
    this.scene.add(this.poolScene.root);
    this.scene.add(this.checkersScene.root);

    this.onPointerDownBound = (ev: PointerEvent): void => {
      if (ev.button === 2 && this.activeGame === "pool") {
        this.poolScene.cancelSlingshot();
        return;
      }
      this.canvas.focus();
      this.canvas.setPointerCapture(ev.pointerId);
      const [wx, wy] = this.clientToWorld(ev.clientX, ev.clientY);
      if (this.activeGame === "checkers") {
        this.checkersScene.handlePointerDown(wx, wy, ev.clientX, ev.clientY);
      } else if (this.activeGame === "pool") {
        this.poolScene.handlePointerDown(wx, wy);
      }
    };
    this.onPointerMoveBound = (ev: PointerEvent): void => {
      const [wx, wy] = this.clientToWorld(ev.clientX, ev.clientY);
      if (this.activeGame === "checkers") {
        this.checkersScene.handlePointerMove(wx, wy, ev.clientX, ev.clientY);
      } else if (this.activeGame === "pool") {
        this.poolScene.handlePointerMove(wx, wy);
      }
    };
    this.onPointerUpBound = (ev: PointerEvent): void => {
      if (this.canvas.hasPointerCapture(ev.pointerId)) {
        this.canvas.releasePointerCapture(ev.pointerId);
      }
      const [wx, wy] = this.clientToWorld(ev.clientX, ev.clientY);
      if (this.activeGame === "checkers") {
        this.checkersScene.handlePointerUp(wx, wy);
      } else if (this.activeGame === "pool") {
        this.poolScene.handlePointerUp(wx, wy);
      }
    };
    this.onKeyDownBound = (ev: KeyboardEvent): void => {
      if (this.activeGame === "checkers" && this.checkersScene.handleKeyDown(ev.key)) {
        ev.preventDefault();
      } else if (this.activeGame === "pool" && ev.key === "Escape") {
        this.poolScene.cancelSlingshot();
        ev.preventDefault();
      }
    };

    this.canvas.addEventListener("pointerdown", this.onPointerDownBound);
    this.canvas.addEventListener("pointermove", this.onPointerMoveBound);
    this.canvas.addEventListener("pointerup", this.onPointerUpBound);
    this.canvas.addEventListener("keydown", this.onKeyDownBound);

    this.resizeObserver = new ResizeObserver(() => {
      this.resize(this.container);
    });
    this.resizeObserver.observe(container);

    this.resize(container);
  }

  public setGame(game: GameKindDto): void {
    this.activeGame = game;
    this.poolScene.root.visible = game === "pool";
    this.checkersScene.root.visible = game === "checkers";
    this.scene.background = new THREE.Color(game === "pool" ? 0x182b1c : 0x3a2a1e);
    this.updateCameraProjection(this.lastAspect);
  }

  public syncCheckers(view: CheckersView): void {
    this.checkersScene.sync(view);
  }

  public syncPool(view: PoolView): void {
    this.poolScene.syncView(view);
  }

  public resize(container: HTMLElement): void {
    const rect = container.getBoundingClientRect();
    const width = Math.max(1, Math.round(rect.width || container.clientWidth));
    const height = Math.max(1, Math.round(rect.height || container.clientHeight));
    this.lastAspect = width / height;
    this.renderer.setSize(width, height, false);
    this.updateCameraProjection(this.lastAspect);
  }

  public render(poolFrame?: Float64Array): void {
    if (this.activeGame === "pool" && poolFrame !== undefined) {
      this.poolScene.syncBallsFromBuffer(poolFrame);
    }
    this.renderer.render(this.scene, this.camera);
  }

  public dispose(): void {
    this.resizeObserver.disconnect();
    this.canvas.removeEventListener("pointerdown", this.onPointerDownBound);
    this.canvas.removeEventListener("pointermove", this.onPointerMoveBound);
    this.canvas.removeEventListener("pointerup", this.onPointerUpBound);
    this.canvas.removeEventListener("keydown", this.onKeyDownBound);
    this.renderer.dispose();
  }

  private clientToWorld(clientX: number, clientY: number): readonly [number, number] {
    const rect = this.canvas.getBoundingClientRect();
    const nx = rect.width > 0 ? (clientX - rect.left) / rect.width : 0.5;
    const ny = rect.height > 0 ? (clientY - rect.top) / rect.height : 0.5;
    const worldX =
      this.camera.position.x + this.camera.left + nx * (this.camera.right - this.camera.left);
    const worldY =
      this.camera.position.y + this.camera.top - ny * (this.camera.top - this.camera.bottom);
    return [worldX, worldY];
  }

  private updateCameraProjection(aspect: number): void {
    const worldW =
      this.activeGame === "checkers"
        ? 8.9
        : POOL_TABLE_WIDTH + (POOL_RAIL_MARGIN + POOL_STAGE_BORDER) * 2;
    const worldH =
      this.activeGame === "checkers"
        ? 8.9
        : POOL_TABLE_HEIGHT + (POOL_RAIL_MARGIN + POOL_STAGE_BORDER) * 2;
    const worldAspect = worldW / worldH;

    let halfW = worldW / 2;
    let halfH = worldH / 2;
    if (aspect > worldAspect) {
      halfW = halfH * aspect;
    } else {
      halfH = halfW / aspect;
    }

    const cx = this.activeGame === "checkers" ? 4.0 : POOL_TABLE_WIDTH / 2;
    const cy = this.activeGame === "checkers" ? 4.0 : POOL_TABLE_HEIGHT / 2;

    this.camera.position.set(cx, cy, 5);
    this.camera.lookAt(cx, cy, 0);
    this.camera.left = -halfW;
    this.camera.right = halfW;
    this.camera.top = halfH;
    this.camera.bottom = -halfH;
    this.camera.updateProjectionMatrix();
  }
}
