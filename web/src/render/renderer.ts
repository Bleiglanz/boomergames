import * as THREE from "three";

import { CheckersScene, type CheckersBoardCallbacks } from "./checkers_scene.js";
import type { CheckersView, GameKindDto } from "../wasm_bridge.js";

const TABLE_WIDTH = 2.54;
const TABLE_HEIGHT = 1.27;
const RAIL_MARGIN = 0.16;
const BALL_RADIUS = 0.028575;
const BALL_STRIDE = 4;
const BALL_COUNT = 16;

const BALL_COLORS: readonly number[] = [
  0xffffff, // 0 cue
  0xf5d020, // 1 yellow
  0x1d4fd8, // 2 blue
  0xd62828, // 3 red
  0x6b2d8b, // 4 purple
  0xf07818, // 5 orange
  0x1d7a36, // 6 green
  0x7a1c1c, // 7 maroon
  0x111111, // 8 black
  0xf5d020, // 9 yellow stripe
  0x1d4fd8, // 10 blue stripe
  0xd62828, // 11 red stripe
  0x6b2d8b, // 12 purple stripe
  0xf07818, // 13 orange stripe
  0x1d7a36, // 14 green stripe
  0x7a1c1c, // 15 maroon stripe
];

export class StageRenderer {
  private readonly renderer: THREE.WebGLRenderer;
  private readonly camera: THREE.OrthographicCamera;
  private readonly scene: THREE.Scene;
  private readonly poolGroup: THREE.Group;
  private readonly checkersScene: CheckersScene;
  private readonly ballMeshes: THREE.Group[] = [];
  private activeGame: GameKindDto | null = null;
  private readonly canvas: HTMLCanvasElement;
  private readonly container: HTMLElement;
  private readonly resizeObserver: ResizeObserver;
  private lastAspect = 1;

  private readonly onPointerDownBound: (ev: PointerEvent) => void;
  private readonly onPointerMoveBound: (ev: PointerEvent) => void;
  private readonly onPointerUpBound: (ev: PointerEvent) => void;
  private readonly onKeyDownBound: (ev: KeyboardEvent) => void;

  public constructor(container: HTMLElement, checkersCallbacks: CheckersBoardCallbacks) {
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

    this.poolGroup = this.buildPoolStage();
    this.checkersScene = new CheckersScene(checkersCallbacks);
    this.scene.add(this.poolGroup);
    this.scene.add(this.checkersScene.root);

    this.onPointerDownBound = (ev: PointerEvent): void => {
      if (this.activeGame !== "checkers") {
        return;
      }
      this.canvas.focus();
      this.canvas.setPointerCapture(ev.pointerId);
      const [wx, wy] = this.clientToWorld(ev.clientX, ev.clientY);
      this.checkersScene.handlePointerDown(wx, wy, ev.clientX, ev.clientY);
    };
    this.onPointerMoveBound = (ev: PointerEvent): void => {
      if (this.activeGame !== "checkers") {
        return;
      }
      const [wx, wy] = this.clientToWorld(ev.clientX, ev.clientY);
      this.checkersScene.handlePointerMove(wx, wy, ev.clientX, ev.clientY);
    };
    this.onPointerUpBound = (ev: PointerEvent): void => {
      if (this.activeGame !== "checkers") {
        return;
      }
      if (this.canvas.hasPointerCapture(ev.pointerId)) {
        this.canvas.releasePointerCapture(ev.pointerId);
      }
      const [wx, wy] = this.clientToWorld(ev.clientX, ev.clientY);
      this.checkersScene.handlePointerUp(wx, wy);
    };
    this.onKeyDownBound = (ev: KeyboardEvent): void => {
      if (this.activeGame === "checkers" && this.checkersScene.handleKeyDown(ev.key)) {
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
    this.poolGroup.visible = game === "pool";
    this.checkersScene.root.visible = game === "checkers";
    this.scene.background = new THREE.Color(game === "pool" ? 0x264129 : 0x3a2a1e);
    this.updateCameraProjection(this.lastAspect);
  }

  public syncCheckers(view: CheckersView): void {
    this.checkersScene.sync(view);
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
      this.syncPoolBalls(poolFrame);
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
    const worldW = this.activeGame === "checkers" ? 8.9 : TABLE_WIDTH + RAIL_MARGIN * 2.4;
    const worldH = this.activeGame === "checkers" ? 8.9 : TABLE_HEIGHT + RAIL_MARGIN * 2.4;
    const worldAspect = worldW / worldH;

    let halfW = worldW / 2;
    let halfH = worldH / 2;
    if (aspect > worldAspect) {
      halfW = halfH * aspect;
    } else {
      halfH = halfW / aspect;
    }

    const cx = this.activeGame === "checkers" ? 4.0 : TABLE_WIDTH / 2;
    const cy = this.activeGame === "checkers" ? 4.0 : TABLE_HEIGHT / 2;

    this.camera.position.set(cx, cy, 5);
    this.camera.lookAt(cx, cy, 0);
    this.camera.left = -halfW;
    this.camera.right = halfW;
    this.camera.top = halfH;
    this.camera.bottom = -halfH;
    this.camera.updateProjectionMatrix();
  }

  private buildPoolStage(): THREE.Group {
    const group = new THREE.Group();

    const railMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(TABLE_WIDTH + RAIL_MARGIN * 2, TABLE_HEIGHT + RAIL_MARGIN * 2),
      new THREE.MeshBasicMaterial({ color: 0x7c1717 }),
    );
    railMesh.position.set(TABLE_WIDTH / 2, TABLE_HEIGHT / 2, 0.0);
    group.add(railMesh);

    const cushionMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(TABLE_WIDTH + 0.06, TABLE_HEIGHT + 0.06),
      new THREE.MeshBasicMaterial({ color: 0x1e661e }),
    );
    cushionMesh.position.set(TABLE_WIDTH / 2, TABLE_HEIGHT / 2, 0.05);
    group.add(cushionMesh);

    const clothMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(TABLE_WIDTH, TABLE_HEIGHT),
      new THREE.MeshBasicMaterial({ color: 0x2e8b2e }),
    );
    clothMesh.position.set(TABLE_WIDTH / 2, TABLE_HEIGHT / 2, 0.1);
    group.add(clothMesh);

    const headLine = new THREE.Mesh(
      new THREE.PlaneGeometry(0.004, TABLE_HEIGHT),
      new THREE.MeshBasicMaterial({ color: 0x62b262 }),
    );
    headLine.position.set(0.635, TABLE_HEIGHT / 2, 0.15);
    group.add(headLine);

    const pocketCoords: readonly [number, number][] = [
      [0, 0],
      [TABLE_WIDTH / 2, -0.02],
      [TABLE_WIDTH, 0],
      [0, TABLE_HEIGHT],
      [TABLE_WIDTH / 2, TABLE_HEIGHT + 0.02],
      [TABLE_WIDTH, TABLE_HEIGHT],
    ];
    const capGeo = new THREE.CircleGeometry(0.072, 24);
    const capMat = new THREE.MeshBasicMaterial({ color: 0xd49b4b });
    const pocketGeo = new THREE.CircleGeometry(0.056, 24);
    const pocketMat = new THREE.MeshBasicMaterial({ color: 0x0b0b0b });

    for (const [px, py] of pocketCoords) {
      const cap = new THREE.Mesh(capGeo, capMat);
      cap.position.set(px, py, 0.18);
      group.add(cap);

      const pocket = new THREE.Mesh(pocketGeo, pocketMat);
      pocket.position.set(px, py, 0.2);
      group.add(pocket);
    }

    const ballGeo = new THREE.CircleGeometry(BALL_RADIUS, 24);
    const stripeGeo = new THREE.PlaneGeometry(BALL_RADIUS * 1.9, BALL_RADIUS * 0.95);
    const dotGeo = new THREE.CircleGeometry(BALL_RADIUS * 0.42, 16);
    const whiteMat = new THREE.MeshBasicMaterial({ color: 0xffffff });

    for (let id = 0; id < BALL_COUNT; id += 1) {
      const ball = new THREE.Group();
      const color = BALL_COLORS[id] ?? 0xffffff;
      const isStripe = id >= 9;

      const base = new THREE.Mesh(
        ballGeo,
        new THREE.MeshBasicMaterial({ color: isStripe ? 0xffffff : color }),
      );
      ball.add(base);

      if (isStripe) {
        const band = new THREE.Mesh(stripeGeo, new THREE.MeshBasicMaterial({ color }));
        band.position.z = 0.02;
        ball.add(band);
      }

      if (id > 0) {
        const centerDot = new THREE.Mesh(dotGeo, whiteMat);
        centerDot.position.z = 0.04;
        ball.add(centerDot);
      }

      ball.position.z = 0.4;
      ball.visible = false;
      group.add(ball);
      this.ballMeshes.push(ball);
    }

    return group;
  }

  private syncPoolBalls(frame: Float64Array): void {
    for (let i = 0; i < BALL_COUNT; i += 1) {
      const mesh = this.ballMeshes[i];
      if (mesh === undefined) {
        continue;
      }
      const base = i * BALL_STRIDE;
      const x = frame[base + 1] ?? 0;
      const y = frame[base + 2] ?? 0;
      const visible = (frame[base + 3] ?? 0) > 0.5;
      mesh.visible = visible;
      if (visible) {
        mesh.position.set(x, y, 0.4);
      }
    }
  }
}
