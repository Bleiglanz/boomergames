import * as THREE from "three";

import type { PoolView } from "../wasm_bridge.js";

export const POOL_TABLE_WIDTH = 2.54;
export const POOL_TABLE_HEIGHT = 1.27;
export const POOL_RAIL_MARGIN = 0.16;
export const POOL_STAGE_BORDER = 0.34;
export const BALL_RADIUS = 0.028575;
const BALL_STRIDE = 4;
const BALL_COUNT = 16;

const BALL_HEX: readonly string[] = [
  "#f8f8f4", // 0 cue
  "#f2c91d", // 1 yellow
  "#1e52d8", // 2 blue
  "#d62828", // 3 red
  "#6e2c91", // 4 purple
  "#f27413", // 5 orange
  "#1a7f37", // 6 green
  "#7b1e1e", // 7 maroon
  "#161616", // 8 black
  "#f2c91d", // 9
  "#1e52d8", // 10
  "#d62828", // 11
  "#6e2c91", // 12
  "#f27413", // 13
  "#1a7f37", // 14
  "#7b1e1e", // 15
];

const POCKET_COORDS: readonly (readonly [number, number])[] = [
  [0, 0],
  [POOL_TABLE_WIDTH / 2, -0.018],
  [POOL_TABLE_WIDTH, 0],
  [0, POOL_TABLE_HEIGHT],
  [POOL_TABLE_WIDTH / 2, POOL_TABLE_HEIGHT + 0.018],
  [POOL_TABLE_WIDTH, POOL_TABLE_HEIGHT],
];

export interface PoolTableCallbacks {
  readonly onAim: (angle: number) => void;
  readonly onPower: (power: number) => void;
  readonly onShoot: () => void;
  readonly onPreviewCue: (x: number, y: number) => void;
  readonly onPlaceCue: (x: number, y: number) => void;
  readonly onCancelCuePreview: () => void;
  readonly onCallPocket: (pocket: number) => void;
}

/**
 * Generates an equirectangular billiard ball texture where `(u=0.25, v=0.5)` maps directly
 * to local `+Z` (straight up toward the orthogonal camera at rest), keeping numbers razor-sharp
 * in 2D top-down mode while rolling authentically in 3D sphere view.
 */
function createBallTexture3D(id: number): THREE.CanvasTexture {
  const w = 512;
  const h = 256;
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    const color = BALL_HEX[id] ?? "#ffffff";
    const isStripe = id >= 9;

    // Base fill
    ctx.fillStyle = isStripe ? "#f8f8f4" : color;
    ctx.fillRect(0, 0, w, h);

    // Classic horizontal stripe band around the equator for stripes (9-15)
    if (isStripe) {
      ctx.fillStyle = color;
      ctx.fillRect(0, h * 0.25, w, h * 0.5);
      ctx.strokeStyle = "#1d1d1d";
      ctx.lineWidth = 3;
      ctx.beginPath();
      ctx.moveTo(0, h * 0.25);
      ctx.lineTo(w, h * 0.25);
      ctx.moveTo(0, h * 0.75);
      ctx.lineTo(w, h * 0.75);
      ctx.stroke();
    }

    // Crisp white number circles at u=0.25 (+Z top at start) and u=0.75 (-Z opposite side)
    if (id > 0) {
      const r = 42;
      for (const cx of [w * 0.25, w * 0.75]) {
        const cy = h * 0.5;
        ctx.fillStyle = "#ffffff";
        ctx.beginPath();
        ctx.arc(cx, cy, r, 0, Math.PI * 2);
        ctx.fill();

        ctx.strokeStyle = "rgba(0, 0, 0, 0.35)";
        ctx.lineWidth = 2;
        ctx.stroke();

        ctx.fillStyle = "#111111";
        ctx.font = "bold 46px Verdana, Tahoma, sans-serif";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(String(id), cx, cy + 2);
      }
    }
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.anisotropy = 4;
  return tex;
}

function createFloorShadowTexture(): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 256;
  canvas.height = 128;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    const grad = ctx.createRadialGradient(128, 64, 24, 128, 64, 124);
    grad.addColorStop(0, "rgba(0, 0, 0, 0.72)");
    grad.addColorStop(0.65, "rgba(0, 0, 0, 0.42)");
    grad.addColorStop(1, "rgba(0, 0, 0, 0.0)");
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, 256, 128);
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

function createWoodFloorTexture(): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 256;
  canvas.height = 256;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    ctx.fillStyle = "#261b14";
    ctx.fillRect(0, 0, 256, 256);
    for (let y = 0; y < 256; y += 32) {
      ctx.fillStyle = y % 64 === 0 ? "#2c1f17" : "#221812";
      ctx.fillRect(0, y, 256, 30);
      ctx.fillStyle = "#160f0b";
      ctx.fillRect(0, y + 30, 256, 2);
    }
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.wrapS = THREE.RepeatWrapping;
  tex.wrapT = THREE.RepeatWrapping;
  tex.repeat.set(10, 10);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

const SLINGSHOT_CANCEL_RADIUS = 0.06;
const SLINGSHOT_MAX_PULL = 0.62;
const POCKET_DROP_MS = 180;
const POCKET_CAPTURE_DIST = 0.16;
const ROLL_AXIS = new THREE.Vector3();
const ROLL_QUAT = new THREE.Quaternion();

interface PocketDropAnim {
  active: boolean;
  startMs: number;
  startX: number;
  startY: number;
  pocketX: number;
  pocketY: number;
}

function findNearestPocket(x: number, y: number): readonly [number, number] | null {
  let best: readonly [number, number] | null = null;
  let bestDist = POCKET_CAPTURE_DIST;
  for (const coord of POCKET_COORDS) {
    const d = Math.hypot(x - coord[0], y - coord[1]);
    if (d <= bestDist) {
      bestDist = d;
      best = coord;
    }
  }
  return best;
}

export class PoolScene {
  public readonly root = new THREE.Group();

  private readonly ballMeshes: THREE.Mesh[] = [];
  private readonly ballShadows: THREE.Mesh[] = [];
  private readonly lastBallPos: { x: number; y: number; initialized: boolean }[] = [];
  private readonly pocketDrops: PocketDropAnim[] = [];
  private readonly cueGroup = new THREE.Group();
  private readonly cueStick = new THREE.Group();
  private readonly aimLine: THREE.Line;
  private readonly objectLine: THREE.Line;
  private readonly deflectionLine: THREE.Line;
  private readonly ghostRing: THREE.Mesh;
  private readonly cuePreviewMesh: THREE.Group;
  private readonly cuePreviewRing: THREE.Mesh;
  private readonly calledPocketRing: THREE.Mesh;
  private readonly slingshotBandBorder: THREE.Mesh;
  private readonly slingshotBandFill: THREE.Mesh;
  private readonly slingshotAnchorRing: THREE.Mesh;
  private readonly slingshotHandleDot: THREE.Mesh;
  private readonly roomGroup = new THREE.Group();
  private readonly ambientLight: THREE.AmbientLight;
  private readonly keyLight: THREE.DirectionalLight;
  private readonly overheadLight: THREE.PointLight;
  private readonly fillLight: THREE.DirectionalLight;

  private readonly callbacks: PoolTableCallbacks;
  private currentView: PoolView | null = null;
  private draggingCueBall = false;
  private slingshotActive = false;
  private slingshotEngaged = false;
  private slingshotReadyToFire = false;
  private slingshotAnchorX = 0;
  private slingshotAnchorY = 0;
  private cueWorldX = 0.635;
  private cueWorldY = 0.635;
  private visual3DBlend = 0;

  public constructor(callbacks: PoolTableCallbacks) {
    this.callbacks = callbacks;

    // Lighting rig: completely flat & uniform at blend=0 (classic 2D top-down look),
    // smoothly transitioning into warm overhead billiard-hall lighting as 3D tilt increases.
    this.ambientLight = new THREE.AmbientLight(0xffffff, 3.1);
    this.keyLight = new THREE.DirectionalLight(0xfff5e4, 0.0);
    this.keyLight.position.set(POOL_TABLE_WIDTH * 0.35, POOL_TABLE_HEIGHT * 0.2, 2.8);
    this.fillLight = new THREE.DirectionalLight(0xdbeafe, 0.0);
    this.fillLight.position.set(POOL_TABLE_WIDTH * 0.9, POOL_TABLE_HEIGHT * 1.4, 1.8);
    this.overheadLight = new THREE.PointLight(0xfff4d6, 0.0, 9.0, 1.2);
    this.overheadLight.position.set(POOL_TABLE_WIDTH / 2, POOL_TABLE_HEIGHT / 2, 1.75);
    this.root.add(this.ambientLight, this.keyLight, this.fillLight, this.overheadLight);

    this.buildTable3D();
    this.buildRoomBackdrop();

    // Real 3D billiard balls (SphereGeometry) + grounded contact shadow discs
    const ballGeo = new THREE.SphereGeometry(BALL_RADIUS, 28, 20);
    const shadowGeo = new THREE.CircleGeometry(BALL_RADIUS * 1.15, 20);
    const shadowMat = new THREE.MeshBasicMaterial({
      color: 0x051408,
      transparent: true,
      opacity: 0.38,
      depthWrite: false,
    });

    for (let id = 0; id < BALL_COUNT; id += 1) {
      const mat = new THREE.MeshStandardMaterial({
        map: createBallTexture3D(id),
        roughness: 0.16,
        metalness: 0.06,
      });
      const mesh = new THREE.Mesh(ballGeo, mat);
      mesh.position.z = BALL_RADIUS;
      mesh.visible = false;
      this.root.add(mesh);
      this.ballMeshes.push(mesh);

      const shadow = new THREE.Mesh(shadowGeo, shadowMat);
      shadow.position.z = 0.002;
      shadow.visible = false;
      this.root.add(shadow);
      this.ballShadows.push(shadow);

      this.lastBallPos.push({ x: 0, y: 0, initialized: false });
      this.pocketDrops.push({
        active: false,
        startMs: 0,
        startX: 0,
        startY: 0,
        pocketX: 0,
        pocketY: 0,
      });
    }

    // Real 3D tapered wooden cue stick (cylinders oriented along -X)
    const shaftGeo = new THREE.CylinderGeometry(0.0065, 0.0105, 0.78, 14);
    shaftGeo.rotateZ(-Math.PI / 2);
    const cueShaft = new THREE.Mesh(
      shaftGeo,
      new THREE.MeshStandardMaterial({ color: 0xe6c48a, roughness: 0.38, metalness: 0.05 }),
    );
    cueShaft.position.set(-0.46, 0, 0);

    const buttGeo = new THREE.CylinderGeometry(0.0105, 0.0145, 0.42, 14);
    buttGeo.rotateZ(-Math.PI / 2);
    const cueButt = new THREE.Mesh(
      buttGeo,
      new THREE.MeshStandardMaterial({ color: 0x4f210e, roughness: 0.3, metalness: 0.08 }),
    );
    cueButt.position.set(-1.05, 0, 0);

    const ferruleGeo = new THREE.CylinderGeometry(0.0062, 0.0066, 0.022, 12);
    ferruleGeo.rotateZ(-Math.PI / 2);
    const cueFerrule = new THREE.Mesh(
      ferruleGeo,
      new THREE.MeshStandardMaterial({ color: 0xf4f4ef, roughness: 0.25 }),
    );
    cueFerrule.position.set(-0.062, 0, 0);

    const tipGeo = new THREE.CylinderGeometry(0.0058, 0.0062, 0.012, 12);
    tipGeo.rotateZ(-Math.PI / 2);
    const cueTip = new THREE.Mesh(
      tipGeo,
      new THREE.MeshStandardMaterial({ color: 0x2b75bd, roughness: 0.7 }),
    );
    cueTip.position.set(-0.045, 0, 0);

    this.cueStick.add(cueShaft, cueButt, cueFerrule, cueTip);
    this.cueGroup.add(this.cueStick);
    this.cueGroup.visible = false;
    this.root.add(this.cueGroup);

    // Aiming aid lines + ghost ball at ball-center height
    this.aimLine = this.makeLine(0xffffff, BALL_RADIUS);
    this.objectLine = this.makeLine(0xffea55, BALL_RADIUS);
    this.deflectionLine = this.makeLine(0xa7f3d0, BALL_RADIUS);
    this.ghostRing = new THREE.Mesh(
      new THREE.RingGeometry(BALL_RADIUS * 0.88, BALL_RADIUS * 1.03, 28),
      new THREE.MeshBasicMaterial({
        color: 0xffffff,
        transparent: true,
        opacity: 0.85,
        side: THREE.DoubleSide,
      }),
    );
    this.ghostRing.position.z = BALL_RADIUS + 0.002;
    this.ghostRing.visible = false;
    this.root.add(this.aimLine, this.objectLine, this.deflectionLine, this.ghostRing);

    // Ball-in-hand drag preview ring (green valid / red invalid)
    this.cuePreviewMesh = new THREE.Group();
    const previewDisc = new THREE.Mesh(
      new THREE.CircleGeometry(BALL_RADIUS, 24),
      new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.55 }),
    );
    this.cuePreviewRing = new THREE.Mesh(
      new THREE.RingGeometry(BALL_RADIUS * 1.02, BALL_RADIUS * 1.35, 28),
      new THREE.MeshBasicMaterial({ color: 0x22c55e, side: THREE.DoubleSide }),
    );
    this.cuePreviewRing.position.z = 0.002;
    this.cuePreviewMesh.add(previewDisc, this.cuePreviewRing);
    this.cuePreviewMesh.position.z = BALL_RADIUS + 0.032;
    this.cuePreviewMesh.visible = false;
    this.root.add(this.cuePreviewMesh);

    // Called 8-ball pocket marker
    this.calledPocketRing = new THREE.Mesh(
      new THREE.RingGeometry(0.068, 0.088, 28),
      new THREE.MeshBasicMaterial({ color: 0xffea00, side: THREE.DoubleSide }),
    );
    this.calledPocketRing.position.z = 0.056;
    this.calledPocketRing.visible = false;
    this.root.add(this.calledPocketRing);

    // Slingshot pull-back visuals (above rail plane so always visible in 2D & 3D)
    this.slingshotBandBorder = new THREE.Mesh(
      new THREE.PlaneGeometry(1, 0.024),
      new THREE.MeshBasicMaterial({
        color: 0x0b0b0b,
        transparent: true,
        opacity: 0.85,
        side: THREE.DoubleSide,
      }),
    );
    this.slingshotBandBorder.position.z = 0.068;
    this.slingshotBandBorder.visible = false;

    this.slingshotBandFill = new THREE.Mesh(
      new THREE.PlaneGeometry(1, 0.014),
      new THREE.MeshBasicMaterial({ color: 0x22c55e, side: THREE.DoubleSide }),
    );
    this.slingshotBandFill.position.z = 0.07;
    this.slingshotBandFill.visible = false;

    this.slingshotAnchorRing = new THREE.Mesh(
      new THREE.RingGeometry(SLINGSHOT_CANCEL_RADIUS * 0.82, SLINGSHOT_CANCEL_RADIUS, 28),
      new THREE.MeshBasicMaterial({
        color: 0xffffff,
        transparent: true,
        opacity: 0.85,
        side: THREE.DoubleSide,
      }),
    );
    this.slingshotAnchorRing.position.z = 0.072;
    this.slingshotAnchorRing.visible = false;

    this.slingshotHandleDot = new THREE.Mesh(
      new THREE.CircleGeometry(0.026, 20),
      new THREE.MeshBasicMaterial({ color: 0x22c55e, side: THREE.DoubleSide }),
    );
    this.slingshotHandleDot.position.z = 0.074;
    this.slingshotHandleDot.visible = false;

    this.root.add(
      this.slingshotBandBorder,
      this.slingshotBandFill,
      this.slingshotAnchorRing,
      this.slingshotHandleDot,
    );

    this.set3DVisualBlend(0);
  }

  public isSlingshotActive(): boolean {
    return this.slingshotActive || this.draggingCueBall;
  }

  public getCueAimState(): { readonly x: number; readonly y: number; readonly angle: number } | null {
    if (this.currentView === null) {
      return null;
    }
    return {
      x: this.cueWorldX,
      y: this.cueWorldY,
      angle: this.currentView.aimAngle,
    };
  }

  /**
   * Smoothly blends between flat 2D top-down lighting (`blend = 0`) and rich 3D pool-hall
   * lighting + floor backdrop (`blend = 1`) as the player tilts the fullscreen camera.
   */
  public set3DVisualBlend(rawBlend: number): void {
    const t = Math.max(0, Math.min(1, rawBlend));
    this.visual3DBlend = t;
    // Flat uniform lighting at t=0 preserves the classic 2D look; dramatic 3D shading at t>0.
    this.ambientLight.intensity = 3.1 - t * 2.05;
    this.keyLight.intensity = t * 1.9;
    this.fillLight.intensity = t * 0.65;
    this.overheadLight.intensity = t * 3.8;
    this.roomGroup.visible = t > 0.01;
    // Elevate cue stick slightly over the 3D cushion rail when tilted
    this.cueStick.rotation.y = -0.095 * t;
    this.cueStick.position.z = 0.015 * t;
  }

  public syncView(view: PoolView): void {
    this.currentView = view;
    const cueBall = view.state.balls.find((b) => b.id === 0);
    if (cueBall !== undefined && !cueBall.pocketed) {
      this.cueWorldX = cueBall.x;
      this.cueWorldY = cueBall.y;
    }

    // Ball-in-hand drag preview
    if (view.cuePreview !== null && !view.animating) {
      this.cuePreviewMesh.visible = true;
      this.cuePreviewMesh.position.set(
        view.cuePreview.pos[0],
        view.cuePreview.pos[1],
        BALL_RADIUS + 0.032,
      );
      const ringMat = this.cuePreviewRing.material as THREE.MeshBasicMaterial;
      ringMat.color.setHex(view.cuePreview.valid ? 0x22c55e : 0xef4444);
    } else {
      this.cuePreviewMesh.visible = false;
    }

    // Called pocket highlight (only when callEight enabled & on 8-ball)
    if (
      view.state.settings.callEight &&
      view.state.isOnEightBall &&
      view.calledPocket !== null &&
      !view.animating
    ) {
      const p = POCKET_COORDS[view.calledPocket];
      if (p !== undefined) {
        this.calledPocketRing.position.set(p[0], p[1], 0.056);
        this.calledPocketRing.visible = true;
      }
    } else {
      this.calledPocketRing.visible = false;
    }

    // Cue stick & aim preview
    const canAim =
      !view.animating &&
      view.state.outcome === null &&
      !view.state.botThinking &&
      view.aimPreview !== null &&
      view.cuePreview === null;

    if (!canAim || view.aimPreview === null) {
      this.cueGroup.visible = false;
      this.aimLine.visible = false;
      this.objectLine.visible = false;
      this.deflectionLine.visible = false;
      this.ghostRing.visible = false;
      return;
    }

    const preview = view.aimPreview;
    this.cueGroup.visible = true;
    this.cueGroup.position.set(preview.cuePos[0], preview.cuePos[1], BALL_RADIUS + 0.012);
    this.cueGroup.rotation.z = view.aimAngle;
    // Pull cue stick back smoothly proportional to shot power
    const pullback = -0.02 - view.power * 0.24;
    this.cueStick.position.x = pullback;
    this.cueStick.rotation.y = -0.095 * this.visual3DBlend;
    this.cueStick.position.z = 0.015 * this.visual3DBlend;

    this.updateLine(this.aimLine, preview.cuePos, preview.ghostBall);
    this.aimLine.visible = true;

    const showGhost = view.state.settings.aimAid !== "off";
    this.ghostRing.visible = showGhost;
    if (showGhost) {
      this.ghostRing.position.set(preview.ghostBall[0], preview.ghostBall[1], BALL_RADIUS + 0.002);
    }

    if (preview.objectLine !== null) {
      this.updateLine(this.objectLine, preview.objectLine[0], preview.objectLine[1]);
      this.objectLine.visible = true;
    } else {
      this.objectLine.visible = false;
    }

    if (preview.cueDeflectionLine !== null) {
      this.updateLine(
        this.deflectionLine,
        preview.cueDeflectionLine[0],
        preview.cueDeflectionLine[1],
      );
      this.deflectionLine.visible = true;
    } else {
      this.deflectionLine.visible = false;
    }
  }

  public syncBallsFromBuffer(frame: Float64Array, nowMs = performance.now()): void {
    // Detect whole-table resets/re-racks so bulk disappearances do not trigger pocket drops.
    let newlyHidden = 0;
    for (let i = 0; i < BALL_COUNT; i += 1) {
      const prev = this.lastBallPos[i];
      const visible = (frame[i * BALL_STRIDE + 3] ?? 0) > 0.5;
      if (prev !== undefined && prev.initialized && !visible) {
        newlyHidden += 1;
      }
    }
    const allowDrop = newlyHidden <= 4;

    for (let i = 0; i < BALL_COUNT; i += 1) {
      const mesh = this.ballMeshes[i];
      const shadow = this.ballShadows[i];
      const prev = this.lastBallPos[i];
      const drop = this.pocketDrops[i];
      if (mesh === undefined || shadow === undefined || prev === undefined || drop === undefined) {
        continue;
      }
      const base = i * BALL_STRIDE;
      const x = frame[base + 1] ?? 0;
      const y = frame[base + 2] ?? 0;
      const visible = (frame[base + 3] ?? 0) > 0.5;
      if (visible) {
        drop.active = false;
        mesh.visible = true;
        shadow.visible = true;
        if (prev.initialized) {
          const dx = x - prev.x;
          const dy = y - prev.y;
          const dist = Math.hypot(dx, dy);
          if (dist > 0.45) {
            // Large teleport (re-rack / cue ball respot): reset upright orientation
            mesh.quaternion.identity();
          } else if (dist > 1e-5) {
            // Physically accurate rolling rotation on the XY cloth plane
            ROLL_AXIS.set(-dy / dist, dx / dist, 0);
            ROLL_QUAT.setFromAxisAngle(ROLL_AXIS, dist / BALL_RADIUS);
            mesh.quaternion.premultiply(ROLL_QUAT);
          }
        } else {
          mesh.quaternion.identity();
          prev.initialized = true;
        }
        prev.x = x;
        prev.y = y;
        mesh.position.set(x, y, BALL_RADIUS);
        shadow.position.set(x, y, 0.002);
        if (i === 0) {
          this.cueWorldX = x;
          this.cueWorldY = y;
        }
      } else {
        if (prev.initialized) {
          if (allowDrop) {
            const pocket = findNearestPocket(prev.x, prev.y);
            if (pocket !== null) {
              drop.active = true;
              drop.startMs = nowMs;
              drop.startX = prev.x;
              drop.startY = prev.y;
              drop.pocketX = pocket[0];
              drop.pocketY = pocket[1];
            }
          }
          prev.initialized = false;
        }

        if (drop.active) {
          const t = Math.max(0, Math.min(1, (nowMs - drop.startMs) / POCKET_DROP_MS));
          if (t >= 1) {
            drop.active = false;
            mesh.visible = false;
            shadow.visible = false;
          } else {
            // Purely visual drop-down into the recessed pocket chute (zero physics change).
            const pull = t * 0.72;
            const bx = drop.startX + (drop.pocketX - drop.startX) * pull;
            const by = drop.startY + (drop.pocketY - drop.startY) * pull;
            const bz = BALL_RADIUS - t * t * (BALL_RADIUS + 0.055);
            const pdx = drop.pocketX - drop.startX;
            const pdy = drop.pocketY - drop.startY;
            const pdist = Math.hypot(pdx, pdy);
            if (pdist > 1e-4) {
              ROLL_AXIS.set(-pdy / pdist, pdx / pdist, 0);
              ROLL_QUAT.setFromAxisAngle(ROLL_AXIS, 0.06);
              mesh.quaternion.premultiply(ROLL_QUAT);
            }
            mesh.position.set(bx, by, bz);
            mesh.visible = true;
            shadow.visible = false;
          }
        } else {
          mesh.visible = false;
          shadow.visible = false;
        }
      }
    }
  }

  public handlePointerDown(worldX: number, worldY: number): void {
    const view = this.currentView;
    if (
      view === null ||
      view.animating ||
      view.state.outcome !== null ||
      view.state.botThinking ||
      view.aimPreview === null
    ) {
      return;
    }

    // 1. Check if clicking a pocket to call the 8-ball pocket.
    if (view.state.settings.callEight && view.state.isOnEightBall) {
      for (let i = 0; i < POCKET_COORDS.length; i += 1) {
        const coord = POCKET_COORDS[i];
        if (coord !== undefined && Math.hypot(worldX - coord[0], worldY - coord[1]) <= 0.11) {
          this.callbacks.onCallPocket(i);
          return;
        }
      }
    }

    // 2. If ball-in-hand and grabbing directly on the cue ball, drag cue ball placement.
    const grabRadius = this.visual3DBlend > 0.3 ? 0.075 : 0.06;
    if (
      view.state.ballInHand &&
      Math.hypot(worldX - this.cueWorldX, worldY - this.cueWorldY) <= grabRadius
    ) {
      this.draggingCueBall = true;
      this.callbacks.onPreviewCue(worldX, worldY);
      return;
    }

    // 3. Otherwise start a slingshot pull-back anchored at the press point.
    this.slingshotActive = true;
    this.slingshotEngaged = false;
    this.slingshotReadyToFire = false;
    this.slingshotAnchorX = worldX;
    this.slingshotAnchorY = worldY;
    this.slingshotAnchorRing.position.set(worldX, worldY, 0.072);
    this.slingshotAnchorRing.visible = true;
    this.slingshotBandBorder.visible = false;
    this.slingshotBandFill.visible = false;
    this.slingshotHandleDot.visible = false;
  }

  public handlePointerMove(worldX: number, worldY: number): void {
    const view = this.currentView;
    if (
      view === null ||
      view.animating ||
      view.state.outcome !== null ||
      view.state.botThinking ||
      view.aimPreview === null
    ) {
      return;
    }
    if (this.draggingCueBall) {
      this.callbacks.onPreviewCue(worldX, worldY);
      return;
    }
    if (!this.slingshotActive) {
      return;
    }

    const dx = this.slingshotAnchorX - worldX;
    const dy = this.slingshotAnchorY - worldY;
    const pullDist = Math.hypot(dx, dy);

    if (pullDist > SLINGSHOT_CANCEL_RADIUS) {
      this.slingshotEngaged = true;
      this.slingshotReadyToFire = true;
      const angle = Math.atan2(dy, dx);
      const power = Math.max(
        0.04,
        Math.min(1.0, (pullDist - SLINGSHOT_CANCEL_RADIUS) / (SLINGSHOT_MAX_PULL - SLINGSHOT_CANCEL_RADIUS)),
      );
      this.updateSlingshotVisuals(worldX, worldY, power, true);
      this.callbacks.onAim(angle);
      this.callbacks.onPower(power);
    } else {
      this.slingshotReadyToFire = false;
      this.updateSlingshotVisuals(worldX, worldY, 0, false);
    }
  }

  public handlePointerUp(worldX: number, worldY: number): void {
    if (this.draggingCueBall) {
      this.draggingCueBall = false;
      this.callbacks.onPlaceCue(worldX, worldY);
      this.callbacks.onCancelCuePreview();
      return;
    }
    if (!this.slingshotActive) {
      return;
    }

    const engaged = this.slingshotEngaged;
    const fire = this.slingshotReadyToFire;
    this.cancelSlingshot();

    if (fire) {
      this.callbacks.onShoot();
    } else if (!engaged) {
      // Quick click without dragging: aim toward the clicked spot without firing.
      this.emitAimFromPointer(worldX, worldY);
    }
  }

  public cancelSlingshot(): void {
    this.slingshotActive = false;
    this.slingshotEngaged = false;
    this.slingshotReadyToFire = false;
    this.slingshotAnchorRing.visible = false;
    this.slingshotBandBorder.visible = false;
    this.slingshotBandFill.visible = false;
    this.slingshotHandleDot.visible = false;
  }

  private updateSlingshotVisuals(
    worldX: number,
    worldY: number,
    power: number,
    active: boolean,
  ): void {
    if (!active) {
      this.slingshotBandBorder.visible = false;
      this.slingshotBandFill.visible = false;
      this.slingshotHandleDot.visible = false;
      const ringMat = this.slingshotAnchorRing.material as THREE.MeshBasicMaterial;
      ringMat.color.setHex(0xffffff);
      return;
    }
    const colorHex = power < 0.45 ? 0x22c55e : power < 0.8 ? 0xfacc15 : 0xff3b30;
    const fillMat = this.slingshotBandFill.material as THREE.MeshBasicMaterial;
    fillMat.color.setHex(colorHex);
    const dotMat = this.slingshotHandleDot.material as THREE.MeshBasicMaterial;
    dotMat.color.setHex(colorHex);
    const ringMat = this.slingshotAnchorRing.material as THREE.MeshBasicMaterial;
    ringMat.color.setHex(colorHex);

    const dx = worldX - this.slingshotAnchorX;
    const dy = worldY - this.slingshotAnchorY;
    const len = Math.max(0.001, Math.hypot(dx, dy));
    const midX = (this.slingshotAnchorX + worldX) * 0.5;
    const midY = (this.slingshotAnchorY + worldY) * 0.5;
    const rot = Math.atan2(dy, dx);

    this.slingshotBandBorder.position.set(midX, midY, 0.068);
    this.slingshotBandBorder.rotation.z = rot;
    this.slingshotBandBorder.scale.set(len, 1, 1);
    this.slingshotBandBorder.visible = true;

    this.slingshotBandFill.position.set(midX, midY, 0.07);
    this.slingshotBandFill.rotation.z = rot;
    this.slingshotBandFill.scale.set(len, 1, 1);
    this.slingshotBandFill.visible = true;

    this.slingshotHandleDot.position.set(worldX, worldY, 0.074);
    this.slingshotHandleDot.visible = true;
  }

  private emitAimFromPointer(worldX: number, worldY: number): void {
    const dx = worldX - this.cueWorldX;
    const dy = worldY - this.cueWorldY;
    if (Math.hypot(dx, dy) > 0.015) {
      this.callbacks.onAim(Math.atan2(dy, dx));
    }
  }

  private makeLine(color: number, z: number): THREE.Line {
    const geo = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(0, 0, z),
      new THREE.Vector3(0, 0, z),
    ]);
    const mat = new THREE.LineBasicMaterial({ color });
    const line = new THREE.Line(geo, mat);
    line.visible = false;
    return line;
  }

  private updateLine(
    line: THREE.Line,
    a: readonly [number, number],
    b: readonly [number, number],
  ): void {
    const pos = line.geometry.getAttribute("position") as THREE.BufferAttribute;
    const z = pos.getZ(0);
    pos.setXYZ(0, a[0], a[1], z);
    pos.setXYZ(1, b[0], b[1], z);
    pos.needsUpdate = true;
  }

  private buildTable3D(): void {
    const cx = POOL_TABLE_WIDTH / 2;
    const cy = POOL_TABLE_HEIGHT / 2;
    const outerW = POOL_TABLE_WIDTH + POOL_RAIL_MARGIN * 2;
    const outerH = POOL_TABLE_HEIGHT + POOL_RAIL_MARGIN * 2;
    const trimPad = POOL_RAIL_MARGIN * 0.09;

    const darkWoodMat = new THREE.MeshStandardMaterial({
      color: 0x520d0d,
      roughness: 0.36,
      metalness: 0.08,
    });
    const railWoodMat = new THREE.MeshStandardMaterial({
      color: 0x821919,
      roughness: 0.28,
      metalness: 0.08,
    });
    const cushionMat = new THREE.MeshStandardMaterial({
      color: 0x1e661e,
      roughness: 0.78,
      metalness: 0.02,
    });
    const clothMat = new THREE.MeshStandardMaterial({
      color: 0x2e8b2e,
      roughness: 0.88,
      metalness: 0.0,
    });
    const brassMat = new THREE.MeshStandardMaterial({
      color: 0xd99b44,
      roughness: 0.28,
      metalness: 0.55,
    });
    const pocketMat = new THREE.MeshStandardMaterial({
      color: 0x090909,
      roughness: 0.95,
    });

    // 1. Playable slate & green billiard cloth (top face sits cleanly at z = 0.0)
    const clothBed = new THREE.Mesh(
      new THREE.BoxGeometry(POOL_TABLE_WIDTH + 0.08, POOL_TABLE_HEIGHT + 0.08, 0.05),
      clothMat,
    );
    clothBed.position.set(cx, cy, -0.025);
    this.root.add(clothBed);

    // Subtle head-string line + foot spot dot flush on cloth (z = 0.001)
    const headLine = new THREE.Mesh(
      new THREE.PlaneGeometry(0.004, POOL_TABLE_HEIGHT),
      new THREE.MeshBasicMaterial({ color: 0x5ab05a }),
    );
    headLine.position.set(0.635, cy, 0.001);
    this.root.add(headLine);

    const footSpot = new THREE.Mesh(
      new THREE.CircleGeometry(0.011, 16),
      new THREE.MeshBasicMaterial({ color: 0x5ab05a }),
    );
    footSpot.position.set(1.905, cy, 0.001);
    this.root.add(footSpot);

    // 2. Raised 3D inner green cushions (z = 0 .. 0.036, cleanly cut around pockets)
    const cushThick = 0.032;
    const cushHeight = 0.036;
    const cushZ = cushHeight / 2;
    const halfSpan = POOL_TABLE_WIDTH / 2 - 0.138;
    const leftCenterX = 0.072 + halfSpan / 2;
    const rightCenterX = POOL_TABLE_WIDTH - 0.072 - halfSpan / 2;
    for (const segX of [leftCenterX, rightCenterX]) {
      for (const y of [-cushThick / 2, POOL_TABLE_HEIGHT + cushThick / 2]) {
        const cush = new THREE.Mesh(
          new THREE.BoxGeometry(halfSpan, cushThick, cushHeight),
          cushionMat,
        );
        cush.position.set(segX, y, cushZ);
        this.root.add(cush);
      }
    }
    const endSpan = POOL_TABLE_HEIGHT - 0.144;
    for (const x of [-cushThick / 2, POOL_TABLE_WIDTH + cushThick / 2]) {
      const cush = new THREE.Mesh(
        new THREE.BoxGeometry(cushThick, endSpan, cushHeight),
        cushionMat,
      );
      cush.position.set(x, cy, cushZ);
      this.root.add(cush);
    }

    // 3. Raised 3D mahogany rail frame (split around pocket openings so pockets are truly open!)
    const railW = POOL_RAIL_MARGIN - cushThick;
    const railH = 0.056;
    const railZ = 0.02; // spans z = -0.008 .. +0.048
    const railHalfSpan = POOL_TABLE_WIDTH / 2 - 0.124;
    const railLeftX = 0.062 + railHalfSpan / 2;
    const railRightX = POOL_TABLE_WIDTH - 0.062 - railHalfSpan / 2;
    // Top & bottom mahogany rail segments (left and right of side pockets)
    for (const segX of [railLeftX, railRightX]) {
      for (const y of [-cushThick - railW / 2, POOL_TABLE_HEIGHT + cushThick + railW / 2]) {
        const beam = new THREE.Mesh(
          new THREE.BoxGeometry(railHalfSpan, railW, railH),
          railWoodMat,
        );
        beam.position.set(segX, y, railZ);
        this.root.add(beam);
      }
    }
    // Left & right mahogany rail segments (between corner pockets)
    const railEndSpan = POOL_TABLE_HEIGHT - 0.124;
    for (const x of [-cushThick - railW / 2, POOL_TABLE_WIDTH + cushThick + railW / 2]) {
      const beam = new THREE.Mesh(new THREE.BoxGeometry(railW, railEndSpan, railH), railWoodMat);
      beam.position.set(x, cy, railZ);
      this.root.add(beam);
    }

    // Outer dark mahogany perimeter trim skirt (frames the rails cleanly in 2D & 3D)
    const trimH = 0.066;
    const trimZ = 0.011; // spans z = -0.022 .. +0.044
    for (const y of [
      -POOL_RAIL_MARGIN - trimPad / 2,
      POOL_TABLE_HEIGHT + POOL_RAIL_MARGIN + trimPad / 2,
    ]) {
      const trim = new THREE.Mesh(
        new THREE.BoxGeometry(outerW + trimPad * 2, trimPad, trimH),
        darkWoodMat,
      );
      trim.position.set(cx, y, trimZ);
      this.root.add(trim);
    }
    for (const x of [
      -POOL_RAIL_MARGIN - trimPad / 2,
      POOL_TABLE_WIDTH + POOL_RAIL_MARGIN + trimPad / 2,
    ]) {
      const trim = new THREE.Mesh(new THREE.BoxGeometry(trimPad, outerH, trimH), darkWoodMat);
      trim.position.set(x, cy, trimZ);
      this.root.add(trim);
    }

    // 4. Classic inlaid rail sight diamonds flush on top of mahogany rails (z = 0.049)
    const diamondGeo = new THREE.CircleGeometry(0.014, 4);
    const diamondMat = new THREE.MeshStandardMaterial({
      color: 0xe6b873,
      roughness: 0.3,
      metalness: 0.45,
    });
    const railOffset = POOL_RAIL_MARGIN * 0.56;
    for (const frac of [1 / 8, 2 / 8, 3 / 8, 5 / 8, 6 / 8, 7 / 8]) {
      const x = POOL_TABLE_WIDTH * frac;
      for (const y of [-railOffset, POOL_TABLE_HEIGHT + railOffset]) {
        const d = new THREE.Mesh(diamondGeo, diamondMat);
        d.position.set(x, y, 0.0495);
        this.root.add(d);
      }
    }
    for (const frac of [0.25, 0.5, 0.75]) {
      const y = POOL_TABLE_HEIGHT * frac;
      for (const x of [-railOffset, POOL_TABLE_WIDTH + railOffset]) {
        const d = new THREE.Mesh(diamondGeo, diamondMat);
        d.position.set(x, y, 0.0495);
        this.root.add(d);
      }
    }

    // 5. 6 open-throat 3D pockets: flush opening at cloth level (z=0.0015) + recessed drop well
    //    + flush corner/side brass castings wrapping ONLY the outer rail perimeter (mouth wide open!)
    const pocketMouthGeo = new THREE.CircleGeometry(0.058, 28);
    const pocketWellWallGeo = new THREE.CylinderGeometry(0.058, 0.054, 0.08, 24, 1, true);
    pocketWellWallGeo.rotateX(Math.PI / 2);

    // Reusable bottom-left corner casting (open toward +X, +Y cloth; flush with rails & outer trim)
    const cornerShape = new THREE.Shape();
    cornerShape.moveTo(0.058, 0);
    cornerShape.lineTo(0.064, -cushThick);
    cornerShape.lineTo(0.064, -POOL_RAIL_MARGIN);
    cornerShape.lineTo(-POOL_RAIL_MARGIN, -POOL_RAIL_MARGIN);
    cornerShape.lineTo(-POOL_RAIL_MARGIN, 0.064);
    cornerShape.lineTo(-cushThick, 0.064);
    cornerShape.lineTo(0, 0.058);
    cornerShape.absarc(0, 0, 0.058, Math.PI * 0.5, Math.PI * 2, false);
    cornerShape.closePath();
    const cornerCastingGeo = new THREE.ExtrudeGeometry(cornerShape, {
      depth: 0.05,
      bevelEnabled: false,
      curveSegments: 24,
    });

    // Reusable bottom side-pocket casting (open toward +Y cloth; flush with rails & outer trim)
    const sideOuterY = -(POOL_RAIL_MARGIN - 0.018);
    const sideCushY = -(cushThick - 0.018);
    const sideShape = new THREE.Shape();
    sideShape.moveTo(-0.058, 0);
    sideShape.lineTo(-0.064, sideCushY);
    sideShape.lineTo(-0.064, sideOuterY);
    sideShape.lineTo(0.064, sideOuterY);
    sideShape.lineTo(0.064, sideCushY);
    sideShape.lineTo(0.058, 0);
    sideShape.absarc(0, 0, 0.058, 0, -Math.PI, true);
    sideShape.closePath();
    const sideCastingGeo = new THREE.ExtrudeGeometry(sideShape, {
      depth: 0.05,
      bevelEnabled: false,
      curveSegments: 24,
    });

    const cornerSpecs: readonly (readonly [number, number, number])[] = [
      [0, 0, 0],
      [POOL_TABLE_WIDTH, 0, Math.PI * 0.5],
      [POOL_TABLE_WIDTH, POOL_TABLE_HEIGHT, Math.PI],
      [0, POOL_TABLE_HEIGHT, Math.PI * 1.5],
    ];
    for (const [px, py, rotZ] of cornerSpecs) {
      const mouth = new THREE.Mesh(pocketMouthGeo, pocketMat);
      mouth.position.set(px, py, 0.0015);
      this.root.add(mouth);

      const well = new THREE.Mesh(pocketWellWallGeo, pocketMat);
      well.position.set(px, py, -0.04);
      this.root.add(well);

      const casting = new THREE.Mesh(cornerCastingGeo, brassMat);
      casting.position.set(px, py, -0.001);
      casting.rotation.z = rotZ;
      this.root.add(casting);
    }

    const sideSpecs: readonly (readonly [number, number, number])[] = [
      [POOL_TABLE_WIDTH / 2, -0.018, 0],
      [POOL_TABLE_WIDTH / 2, POOL_TABLE_HEIGHT + 0.018, Math.PI],
    ];
    for (const [px, py, rotZ] of sideSpecs) {
      const mouth = new THREE.Mesh(pocketMouthGeo, pocketMat);
      mouth.position.set(px, py, 0.0015);
      this.root.add(mouth);

      const well = new THREE.Mesh(pocketWellWallGeo, pocketMat);
      well.position.set(px, py, -0.04);
      this.root.add(well);

      const casting = new THREE.Mesh(sideCastingGeo, brassMat);
      casting.position.set(px, py, -0.001);
      casting.rotation.z = rotZ;
      this.root.add(casting);
    }

    // 6. Hidden under-table 3D mahogany cabinet apron & 4 carved legs (100% occluded from straight +Z!)
    const apron = new THREE.Mesh(
      new THREE.BoxGeometry(POOL_TABLE_WIDTH + 0.14, POOL_TABLE_HEIGHT + 0.14, 0.22),
      darkWoodMat,
    );
    apron.position.set(cx, cy, -0.15);
    this.root.add(apron);

    const legGeo = new THREE.CylinderGeometry(0.072, 0.048, 0.58, 16);
    legGeo.rotateX(Math.PI / 2);
    const footGeo = new THREE.CylinderGeometry(0.054, 0.058, 0.05, 16);
    footGeo.rotateX(Math.PI / 2);

    const legInsetX = 0.12;
    const legInsetY = 0.08;
    const legPositions: readonly (readonly [number, number])[] = [
      [legInsetX, legInsetY],
      [POOL_TABLE_WIDTH - legInsetX, legInsetY],
      [legInsetX, POOL_TABLE_HEIGHT - legInsetY],
      [POOL_TABLE_WIDTH - legInsetX, POOL_TABLE_HEIGHT - legInsetY],
    ];
    for (const [lx, ly] of legPositions) {
      const leg = new THREE.Mesh(legGeo, railWoodMat);
      leg.position.set(lx, ly, -0.55);
      this.root.add(leg);

      const foot = new THREE.Mesh(footGeo, brassMat);
      foot.position.set(lx, ly, -0.855);
      this.root.add(foot);
    }
  }

  private buildRoomBackdrop(): void {
    const cx = POOL_TABLE_WIDTH / 2;
    const cy = POOL_TABLE_HEIGHT / 2;

    // Warm dark billiard-hall hardwood floor underneath the legs (z = -0.88)
    const floor = new THREE.Mesh(
      new THREE.PlaneGeometry(14, 14),
      new THREE.MeshStandardMaterial({
        map: createWoodFloorTexture(),
        roughness: 0.72,
        metalness: 0.05,
      }),
    );
    floor.position.set(cx, cy, -0.88);
    this.roomGroup.add(floor);

    // Soft ambient shadow cast onto the floor directly beneath the table
    const tableShadow = new THREE.Mesh(
      new THREE.PlaneGeometry(POOL_TABLE_WIDTH + 1.1, POOL_TABLE_HEIGHT + 1.0),
      new THREE.MeshBasicMaterial({
        map: createFloorShadowTexture(),
        transparent: true,
        depthWrite: false,
      }),
    );
    tableShadow.position.set(cx, cy, -0.875);
    this.roomGroup.add(tableShadow);

    this.roomGroup.visible = false;
    this.root.add(this.roomGroup);
  }
}
