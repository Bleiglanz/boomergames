import * as THREE from "three";

import { CheckersScene, type CheckersBoardCallbacks } from "./checkers_scene.js";
import {
  BALL_RADIUS,
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
  readonly onPool3DChange?: (is3D: boolean) => void;
}

const PERSP_FOV_DEG = 42;
const MAX_TILT_RAD = 1.24; // ~71 deg dramatic low table view
const MIN_ZOOM = 0.65;
const MAX_ZOOM = 2.35;
const FELT_PLANE = new THREE.Plane(new THREE.Vector3(0, 0, 1), -BALL_RADIUS);
const CHECKERS_PLANE = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);

export class StageRenderer {
  private readonly renderer: THREE.WebGLRenderer;
  private readonly orthoCamera: THREE.OrthographicCamera;
  private readonly perspCamera: THREE.PerspectiveCamera;
  private readonly raycaster = new THREE.Raycaster();
  private readonly ndc = new THREE.Vector2();
  private readonly hitPoint = new THREE.Vector3();

  private readonly scene: THREE.Scene;
  private readonly poolScene: PoolScene;
  private readonly checkersScene: CheckersScene;
  private readonly onPool3DChange?: (is3D: boolean) => void;
  private activeGame: GameKindDto | null = null;
  private readonly canvas: HTMLCanvasElement;
  private readonly container: HTMLElement;
  private readonly resizeObserver: ResizeObserver;
  private lastAspect = 1;
  private orthoHalfH = 1;

  // Fullscreen hidden 3D camera orbit state (starts strictly at 0 = orthogonal top-down!)
  private poolFullscreen = false;
  private camTilt = 0;
  private camAzimuth = 0;
  private camZoom = 1.0;
  private targetTilt = 0;
  private targetAzimuth = 0;
  private targetZoom = 1.0;
  private lastReported3D = false;

  // Orbit pointer tracking (right-drag, middle-drag, Shift+drag, or 2-finger touch)
  private orbitPointerId: number | null = null;
  private orbitLastX = 0;
  private orbitLastY = 0;
  private readonly activeTouches = new Map<number, { x: number; y: number }>();
  private lastPinchDist: number | null = null;
  private lastPinchCenter: { x: number; y: number } | null = null;

  private readonly onPointerDownBound: (ev: PointerEvent) => void;
  private readonly onPointerMoveBound: (ev: PointerEvent) => void;
  private readonly onPointerUpBound: (ev: PointerEvent) => void;
  private readonly onWheelBound: (ev: WheelEvent) => void;
  private readonly onContextMenuBound: (ev: MouseEvent) => void;
  private readonly onKeyDownBound: (ev: KeyboardEvent) => void;

  public constructor(container: HTMLElement, callbacks: StageCallbacks) {
    this.container = container;
    this.onPool3DChange = callbacks.onPool3DChange;
    this.renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: "default" });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.canvas = this.renderer.domElement;
    this.canvas.tabIndex = 0;
    this.canvas.setAttribute("aria-label", "Interactive game board");
    container.innerHTML = "";
    container.appendChild(this.canvas);

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x1f3a24);
    this.orthoCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 30);
    this.orthoCamera.position.set(0, 0, 5);
    this.orthoCamera.lookAt(0, 0, 0);

    this.perspCamera = new THREE.PerspectiveCamera(PERSP_FOV_DEG, 1, 0.05, 40);

    this.poolScene = new PoolScene(callbacks.pool);
    this.checkersScene = new CheckersScene(callbacks.checkers);
    this.scene.add(this.poolScene.root);
    this.scene.add(this.checkersScene.root);

    this.onContextMenuBound = (ev: MouseEvent): void => {
      if (this.activeGame === "pool") {
        ev.preventDefault();
      }
    };

    this.onPointerDownBound = (ev: PointerEvent): void => {
      if (ev.pointerType === "touch") {
        this.activeTouches.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
        if (this.poolFullscreen && this.activeGame === "pool" && this.activeTouches.size === 2) {
          this.poolScene.cancelSlingshot();
          const pts = Array.from(this.activeTouches.values());
          const a = pts[0];
          const b = pts[1];
          if (a !== undefined && b !== undefined) {
            this.lastPinchDist = Math.hypot(b.x - a.x, b.y - a.y);
            this.lastPinchCenter = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
          }
          return;
        }
      }

      const wantsOrbit =
        this.poolFullscreen &&
        this.activeGame === "pool" &&
        (ev.button === 1 ||
          (ev.button === 2 && !this.poolScene.isSlingshotActive()) ||
          (ev.button === 0 && ev.shiftKey));

      if (wantsOrbit) {
        this.canvas.focus();
        this.canvas.setPointerCapture(ev.pointerId);
        this.orbitPointerId = ev.pointerId;
        this.orbitLastX = ev.clientX;
        this.orbitLastY = ev.clientY;
        return;
      }

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
      if (ev.pointerType === "touch" && this.activeTouches.has(ev.pointerId)) {
        this.activeTouches.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
        if (this.poolFullscreen && this.activeGame === "pool" && this.activeTouches.size === 2) {
          const pts = Array.from(this.activeTouches.values());
          const a = pts[0];
          const b = pts[1];
          if (a !== undefined && b !== undefined) {
            const dist = Math.hypot(b.x - a.x, b.y - a.y);
            const center = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
            if (this.lastPinchCenter !== null) {
              const dx = center.x - this.lastPinchCenter.x;
              const dy = center.y - this.lastPinchCenter.y;
              this.applyOrbitDelta(dx, dy);
            }
            if (this.lastPinchDist !== null && this.lastPinchDist > 10) {
              this.applyZoomFactor(dist / this.lastPinchDist);
            }
            this.lastPinchDist = dist;
            this.lastPinchCenter = center;
          }
          return;
        }
      }

      if (this.orbitPointerId === ev.pointerId) {
        const dx = ev.clientX - this.orbitLastX;
        const dy = ev.clientY - this.orbitLastY;
        this.orbitLastX = ev.clientX;
        this.orbitLastY = ev.clientY;
        this.applyOrbitDelta(dx, dy);
        return;
      }

      const [wx, wy] = this.clientToWorld(ev.clientX, ev.clientY);
      if (this.activeGame === "checkers") {
        this.checkersScene.handlePointerMove(wx, wy, ev.clientX, ev.clientY);
      } else if (this.activeGame === "pool") {
        this.poolScene.handlePointerMove(wx, wy);
      }
    };

    this.onPointerUpBound = (ev: PointerEvent): void => {
      if (ev.pointerType === "touch") {
        this.activeTouches.delete(ev.pointerId);
        if (this.activeTouches.size < 2) {
          this.lastPinchDist = null;
          this.lastPinchCenter = null;
        }
      }
      if (this.canvas.hasPointerCapture(ev.pointerId)) {
        this.canvas.releasePointerCapture(ev.pointerId);
      }
      if (this.orbitPointerId === ev.pointerId) {
        this.orbitPointerId = null;
        return;
      }
      const [wx, wy] = this.clientToWorld(ev.clientX, ev.clientY);
      if (this.activeGame === "checkers") {
        this.checkersScene.handlePointerUp(wx, wy);
      } else if (this.activeGame === "pool") {
        this.poolScene.handlePointerUp(wx, wy);
      }
    };

    this.onWheelBound = (ev: WheelEvent): void => {
      if (!this.poolFullscreen || this.activeGame !== "pool") {
        return;
      }
      ev.preventDefault();
      const factor = ev.deltaY < 0 ? 1.09 : 1 / 1.09;
      this.applyZoomFactor(factor);
      // Scrolling in from pure 2D gently reveals the 3D angle!
      if (ev.deltaY < 0 && this.targetTilt < 0.08) {
        this.targetTilt = 0.42;
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

    this.canvas.addEventListener("contextmenu", this.onContextMenuBound);
    this.canvas.addEventListener("pointerdown", this.onPointerDownBound);
    this.canvas.addEventListener("pointermove", this.onPointerMoveBound);
    this.canvas.addEventListener("pointerup", this.onPointerUpBound);
    this.canvas.addEventListener("pointercancel", this.onPointerUpBound);
    this.canvas.addEventListener("wheel", this.onWheelBound, { passive: false });
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
    if (game !== "pool") {
      this.setPoolFullscreen(false);
    }
    this.updateCameraProjection(this.lastAspect);
  }

  public setPoolFullscreen(enabled: boolean): void {
    this.poolFullscreen = enabled;
    if (!enabled) {
      // Snap cleanly back to classic 2D orthogonal top-down when exiting fullscreen
      this.targetTilt = 0;
      this.targetAzimuth = 0;
      this.targetZoom = 1.0;
      this.camTilt = 0;
      this.camAzimuth = 0;
      this.camZoom = 1.0;
      this.poolScene.set3DVisualBlend(0);
      this.report3DState(false);
    }
    this.updateCameraProjection(this.lastAspect);
  }

  public resetCamera2D(): void {
    this.targetTilt = 0;
    this.targetAzimuth = 0;
    this.targetZoom = 1.0;
  }

  public nudgeCamera3D(dTilt: number, dAzimuth: number): void {
    if (!this.poolFullscreen || this.activeGame !== "pool") {
      return;
    }
    this.targetTilt = Math.max(0, Math.min(MAX_TILT_RAD, this.targetTilt + dTilt));
    this.targetAzimuth += dAzimuth;
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
    if (this.activeGame === "pool") {
      if (poolFrame !== undefined) {
        this.poolScene.syncBallsFromBuffer(poolFrame);
      }
      this.stepCamera3D();
    }
    const cam = this.activeCamera();
    this.renderer.render(this.scene, cam);
  }

  public dispose(): void {
    this.resizeObserver.disconnect();
    this.canvas.removeEventListener("contextmenu", this.onContextMenuBound);
    this.canvas.removeEventListener("pointerdown", this.onPointerDownBound);
    this.canvas.removeEventListener("pointermove", this.onPointerMoveBound);
    this.canvas.removeEventListener("pointerup", this.onPointerUpBound);
    this.canvas.removeEventListener("pointercancel", this.onPointerUpBound);
    this.canvas.removeEventListener("wheel", this.onWheelBound);
    this.canvas.removeEventListener("keydown", this.onKeyDownBound);
    this.renderer.dispose();
  }

  private isUsingPerspective(): boolean {
    return (
      this.activeGame === "pool" &&
      this.poolFullscreen &&
      (this.camTilt > 0.004 || Math.abs(this.camZoom - 1.0) > 0.004 || Math.abs(this.camAzimuth) > 0.004)
    );
  }

  private activeCamera(): THREE.Camera {
    return this.isUsingPerspective() ? this.perspCamera : this.orthoCamera;
  }

  private applyOrbitDelta(dx: number, dy: number): void {
    this.targetAzimuth -= dx * 0.0065;
    this.targetTilt = Math.max(0, Math.min(MAX_TILT_RAD, this.targetTilt + dy * 0.0065));
  }

  private applyZoomFactor(factor: number): void {
    this.targetZoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, this.targetZoom * factor));
  }

  private stepCamera3D(): void {
    const smooth = 0.16;
    this.camTilt += (this.targetTilt - this.camTilt) * smooth;
    this.camAzimuth += (this.targetAzimuth - this.camAzimuth) * smooth;
    this.camZoom += (this.targetZoom - this.camZoom) * smooth;

    if (this.targetTilt === 0 && this.camTilt < 0.003) {
      this.camTilt = 0;
    }
    if (this.targetAzimuth === 0 && Math.abs(this.camAzimuth) < 0.003) {
      this.camAzimuth = 0;
    }
    if (this.targetZoom === 1.0 && Math.abs(this.camZoom - 1.0) < 0.003) {
      this.camZoom = 1.0;
    }

    const in3D = this.isUsingPerspective();
    this.report3DState(in3D);

    if (!in3D) {
      this.poolScene.set3DVisualBlend(0);
      this.scene.background = new THREE.Color(0x182b1c);
      return;
    }

    // Smoothly blend lighting & pool-hall room backdrop as camera tilts off straight +Z
    const blend = Math.min(1, this.camTilt / 0.28);
    this.poolScene.set3DVisualBlend(blend);
    const bg2D = new THREE.Color(0x182b1c);
    const bg3D = new THREE.Color(0x120d0a);
    this.scene.background = bg2D.lerp(bg3D, blend);

    // Singularity-free spherical orbit matching the exact orthographic framing at tilt=0
    const cx = POOL_TABLE_WIDTH / 2;
    const cy = POOL_TABLE_HEIGHT / 2;
    const baseDist = this.orthoHalfH / Math.tan(THREE.MathUtils.degToRad(PERSP_FOV_DEG / 2));
    const dist = baseDist / this.camZoom;

    const sinA = Math.sin(this.camAzimuth);
    const cosA = Math.cos(this.camAzimuth);
    const sinT = Math.sin(this.camTilt);
    const cosT = Math.cos(this.camTilt);

    // Forward vector on the XY plane (screen-UP at tilt=0)
    const fx = -sinA;
    const fy = cosA;

    this.perspCamera.position.set(cx - fx * dist * sinT, cy - fy * dist * sinT, dist * cosT);
    this.perspCamera.up.set(fx * cosT, fy * cosT, sinT);
    this.perspCamera.lookAt(cx, cy, 0);
    this.perspCamera.updateMatrixWorld();
  }

  private report3DState(is3D: boolean): void {
    if (is3D !== this.lastReported3D) {
      this.lastReported3D = is3D;
      this.onPool3DChange?.(is3D);
    }
  }

  private clientToWorld(clientX: number, clientY: number): readonly [number, number] {
    const rect = this.canvas.getBoundingClientRect();
    const nx = rect.width > 0 ? (clientX - rect.left) / rect.width : 0.5;
    const ny = rect.height > 0 ? (clientY - rect.top) / rect.height : 0.5;
    this.ndc.set(nx * 2 - 1, -(ny * 2 - 1));
    this.raycaster.setFromCamera(this.ndc, this.activeCamera());
    const plane = this.activeGame === "checkers" ? CHECKERS_PLANE : FELT_PLANE;
    if (this.raycaster.ray.intersectPlane(plane, this.hitPoint) !== null) {
      return [this.hitPoint.x, this.hitPoint.y];
    }
    return [POOL_TABLE_WIDTH / 2, POOL_TABLE_HEIGHT / 2];
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
    this.orthoHalfH = halfH;

    const cx = this.activeGame === "checkers" ? 4.0 : POOL_TABLE_WIDTH / 2;
    const cy = this.activeGame === "checkers" ? 4.0 : POOL_TABLE_HEIGHT / 2;

    this.orthoCamera.position.set(cx, cy, 5);
    this.orthoCamera.up.set(0, 1, 0);
    this.orthoCamera.lookAt(cx, cy, 0);
    this.orthoCamera.left = -halfW;
    this.orthoCamera.right = halfW;
    this.orthoCamera.top = halfH;
    this.orthoCamera.bottom = -halfH;
    this.orthoCamera.updateProjectionMatrix();

    this.perspCamera.aspect = aspect;
    this.perspCamera.updateProjectionMatrix();
  }
}
