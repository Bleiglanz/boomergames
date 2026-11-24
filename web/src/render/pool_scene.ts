import * as THREE from "three";

import type { PoolView } from "../wasm_bridge.js";

export const POOL_TABLE_WIDTH = 2.54;
export const POOL_TABLE_HEIGHT = 1.27;
export const POOL_RAIL_MARGIN = 0.16;
export const POOL_STAGE_BORDER = 0.34;
const BALL_RADIUS = 0.028575;
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

function createBallTexture(id: number): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 128;
  canvas.height = 128;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    const cx = 64;
    const cy = 64;
    const r = 58;
    const color = BALL_HEX[id] ?? "#ffffff";
    const isStripe = id >= 9;

    ctx.clearRect(0, 0, 128, 128);
    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    ctx.clip();

    // Base fill
    ctx.fillStyle = isStripe ? "#f8f8f4" : color;
    ctx.fillRect(0, 0, 128, 128);

    // Classic horizontal color band for stripes (9-15)
    if (isStripe) {
      ctx.fillStyle = color;
      ctx.fillRect(0, 30, 128, 68);
      ctx.strokeStyle = "#222222";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(0, 30);
      ctx.lineTo(128, 30);
      ctx.moveTo(0, 98);
      ctx.lineTo(128, 98);
      ctx.stroke();
    }

    // White number circle + crisp number (1-15)
    if (id > 0) {
      ctx.fillStyle = "#ffffff";
      ctx.beginPath();
      ctx.arc(cx, cy, 24, 0, Math.PI * 2);
      ctx.fill();

      ctx.fillStyle = "#111111";
      ctx.font = "bold 30px Verdana, Tahoma, sans-serif";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(String(id), cx, cy + 1);
    }

    // Simple classic 2000s flat highlight dot
    ctx.fillStyle = "rgba(255, 255, 255, 0.55)";
    ctx.beginPath();
    ctx.arc(cx - 22, cy - 22, 9, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();

    // Crisp dark rim for high contrast on green felt
    ctx.strokeStyle = "#111111";
    ctx.lineWidth = 4;
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    ctx.stroke();
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

const SLINGSHOT_CANCEL_RADIUS = 0.06;
const SLINGSHOT_MAX_PULL = 0.62;

export class PoolScene {
  public readonly root = new THREE.Group();

  private readonly ballMeshes: THREE.Mesh[] = [];
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

  public constructor(callbacks: PoolTableCallbacks) {
    this.callbacks = callbacks;
    this.buildTable();

    // Balls (0..15)
    const ballGeo = new THREE.PlaneGeometry(BALL_RADIUS * 2.04, BALL_RADIUS * 2.04);
    for (let id = 0; id < BALL_COUNT; id += 1) {
      const mat = new THREE.MeshBasicMaterial({
        map: createBallTexture(id),
        transparent: true,
      });
      const mesh = new THREE.Mesh(ballGeo, mat);
      mesh.position.z = 0.45;
      mesh.visible = false;
      this.root.add(mesh);
      this.ballMeshes.push(mesh);
    }

    // Classic light-wood cue stick
    const cueShaft = new THREE.Mesh(
      new THREE.PlaneGeometry(0.95, 0.015),
      new THREE.MeshBasicMaterial({ color: 0xe6c48a }),
    );
    cueShaft.position.set(-0.55, 0, 0.6);

    const cueButt = new THREE.Mesh(
      new THREE.PlaneGeometry(0.36, 0.022),
      new THREE.MeshBasicMaterial({ color: 0x5c2b12 }),
    );
    cueButt.position.set(-0.88, 0, 0.61);

    const cueTip = new THREE.Mesh(
      new THREE.PlaneGeometry(0.024, 0.015),
      new THREE.MeshBasicMaterial({ color: 0x2b75bd }),
    );
    cueTip.position.set(-0.07, 0, 0.62);

    this.cueStick.add(cueShaft, cueButt, cueTip);
    this.cueGroup.add(this.cueStick);
    this.cueGroup.visible = false;
    this.root.add(this.cueGroup);

    // Aiming aid lines + ghost ball
    this.aimLine = this.makeLine(0xffffff, 0.55);
    this.objectLine = this.makeLine(0xffea55, 0.54);
    this.deflectionLine = this.makeLine(0xa7f3d0, 0.53);
    this.ghostRing = new THREE.Mesh(
      new THREE.RingGeometry(BALL_RADIUS * 0.88, BALL_RADIUS * 1.03, 28),
      new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.85 }),
    );
    this.ghostRing.position.z = 0.56;
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
      new THREE.MeshBasicMaterial({ color: 0x22c55e }),
    );
    this.cuePreviewRing.position.z = 0.02;
    this.cuePreviewMesh.add(previewDisc, this.cuePreviewRing);
    this.cuePreviewMesh.position.z = 0.72;
    this.cuePreviewMesh.visible = false;
    this.root.add(this.cuePreviewMesh);

    // Called 8-ball pocket marker
    this.calledPocketRing = new THREE.Mesh(
      new THREE.RingGeometry(0.068, 0.088, 28),
      new THREE.MeshBasicMaterial({ color: 0xffea00 }),
    );
    this.calledPocketRing.position.z = 0.35;
    this.calledPocketRing.visible = false;
    this.root.add(this.calledPocketRing);

    // Slingshot pull-back visuals (cancel ring + high-contrast outlined band + handle dot)
    this.slingshotBandBorder = new THREE.Mesh(
      new THREE.PlaneGeometry(1, 0.024),
      new THREE.MeshBasicMaterial({ color: 0x0b0b0b, transparent: true, opacity: 0.85 }),
    );
    this.slingshotBandBorder.position.z = 0.74;
    this.slingshotBandBorder.visible = false;

    this.slingshotBandFill = new THREE.Mesh(
      new THREE.PlaneGeometry(1, 0.014),
      new THREE.MeshBasicMaterial({ color: 0x22c55e }),
    );
    this.slingshotBandFill.position.z = 0.75;
    this.slingshotBandFill.visible = false;

    this.slingshotAnchorRing = new THREE.Mesh(
      new THREE.RingGeometry(SLINGSHOT_CANCEL_RADIUS * 0.82, SLINGSHOT_CANCEL_RADIUS, 28),
      new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.85 }),
    );
    this.slingshotAnchorRing.position.z = 0.76;
    this.slingshotAnchorRing.visible = false;

    this.slingshotHandleDot = new THREE.Mesh(
      new THREE.CircleGeometry(0.026, 20),
      new THREE.MeshBasicMaterial({ color: 0x22c55e }),
    );
    this.slingshotHandleDot.position.z = 0.77;
    this.slingshotHandleDot.visible = false;

    this.root.add(
      this.slingshotBandBorder,
      this.slingshotBandFill,
      this.slingshotAnchorRing,
      this.slingshotHandleDot,
    );
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
      this.cuePreviewMesh.position.set(view.cuePreview.pos[0], view.cuePreview.pos[1], 0.72);
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
        this.calledPocketRing.position.set(p[0], p[1], 0.35);
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
    this.cueGroup.position.set(preview.cuePos[0], preview.cuePos[1], 0.0);
    this.cueGroup.rotation.z = view.aimAngle;
    // Pull cue stick back smoothly proportional to shot power
    const pullback = -0.02 - view.power * 0.24;
    this.cueStick.position.x = pullback;

    this.updateLine(this.aimLine, preview.cuePos, preview.ghostBall);
    this.aimLine.visible = true;

    const showGhost = view.state.settings.aimAid !== "off";
    this.ghostRing.visible = showGhost;
    if (showGhost) {
      this.ghostRing.position.set(preview.ghostBall[0], preview.ghostBall[1], 0.56);
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

  public syncBallsFromBuffer(frame: Float64Array): void {
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
        mesh.position.set(x, y, 0.45);
        if (i === 0) {
          this.cueWorldX = x;
          this.cueWorldY = y;
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
    if (
      view.state.ballInHand &&
      Math.hypot(worldX - this.cueWorldX, worldY - this.cueWorldY) <= 0.06
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
    this.slingshotAnchorRing.position.set(worldX, worldY, 0.76);
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

    this.slingshotBandBorder.position.set(midX, midY, 0.74);
    this.slingshotBandBorder.rotation.z = rot;
    this.slingshotBandBorder.scale.set(len, 1, 1);
    this.slingshotBandBorder.visible = true;

    this.slingshotBandFill.position.set(midX, midY, 0.75);
    this.slingshotBandFill.rotation.z = rot;
    this.slingshotBandFill.scale.set(len, 1, 1);
    this.slingshotBandFill.visible = true;

    this.slingshotHandleDot.position.set(worldX, worldY, 0.77);
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

  private buildTable(): void {
    // Outer dark maroon wood trim
    const outerRim = new THREE.Mesh(
      new THREE.PlaneGeometry(
        POOL_TABLE_WIDTH + POOL_RAIL_MARGIN * 2.18,
        POOL_TABLE_HEIGHT + POOL_RAIL_MARGIN * 2.18,
      ),
      new THREE.MeshBasicMaterial({ color: 0x520d0d }),
    );
    outerRim.position.set(POOL_TABLE_WIDTH / 2, POOL_TABLE_HEIGHT / 2, 0.0);
    this.root.add(outerRim);

    // Classic maroon wood rail frame
    const railMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(
        POOL_TABLE_WIDTH + POOL_RAIL_MARGIN * 2,
        POOL_TABLE_HEIGHT + POOL_RAIL_MARGIN * 2,
      ),
      new THREE.MeshBasicMaterial({ color: 0x821919 }),
    );
    railMesh.position.set(POOL_TABLE_WIDTH / 2, POOL_TABLE_HEIGHT / 2, 0.02);
    this.root.add(railMesh);

    // Classic tan rail diamonds (sights)
    const diamondGeo = new THREE.CircleGeometry(0.014, 4);
    const diamondMat = new THREE.MeshBasicMaterial({ color: 0xe6b873 });
    const railOffset = POOL_RAIL_MARGIN * 0.52;
    for (const frac of [1 / 8, 2 / 8, 3 / 8, 5 / 8, 6 / 8, 7 / 8]) {
      const x = POOL_TABLE_WIDTH * frac;
      for (const y of [-railOffset, POOL_TABLE_HEIGHT + railOffset]) {
        const d = new THREE.Mesh(diamondGeo, diamondMat);
        d.position.set(x, y, 0.04);
        this.root.add(d);
      }
    }
    for (const frac of [0.25, 0.5, 0.75]) {
      const y = POOL_TABLE_HEIGHT * frac;
      for (const x of [-railOffset, POOL_TABLE_WIDTH + railOffset]) {
        const d = new THREE.Mesh(diamondGeo, diamondMat);
        d.position.set(x, y, 0.04);
        this.root.add(d);
      }
    }

    // Darker inner cushion edge
    const cushionMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(POOL_TABLE_WIDTH + 0.06, POOL_TABLE_HEIGHT + 0.06),
      new THREE.MeshBasicMaterial({ color: 0x1e661e }),
    );
    cushionMesh.position.set(POOL_TABLE_WIDTH / 2, POOL_TABLE_HEIGHT / 2, 0.06);
    this.root.add(cushionMesh);

    // Classic billiard green cloth (#2E8B2E)
    const clothMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(POOL_TABLE_WIDTH, POOL_TABLE_HEIGHT),
      new THREE.MeshBasicMaterial({ color: 0x2e8b2e }),
    );
    clothMesh.position.set(POOL_TABLE_WIDTH / 2, POOL_TABLE_HEIGHT / 2, 0.1);
    this.root.add(clothMesh);

    // Faint vertical head-string line + foot spot dot
    const headLine = new THREE.Mesh(
      new THREE.PlaneGeometry(0.004, POOL_TABLE_HEIGHT),
      new THREE.MeshBasicMaterial({ color: 0x5ab05a }),
    );
    headLine.position.set(0.635, POOL_TABLE_HEIGHT / 2, 0.14);
    this.root.add(headLine);

    const footSpot = new THREE.Mesh(
      new THREE.CircleGeometry(0.011, 16),
      new THREE.MeshBasicMaterial({ color: 0x5ab05a }),
    );
    footSpot.position.set(1.905, POOL_TABLE_HEIGHT / 2, 0.14);
    this.root.add(footSpot);

    // 6 pockets with classic tan/orange corner & side caps
    const capGeo = new THREE.CircleGeometry(0.076, 24);
    const capMat = new THREE.MeshBasicMaterial({ color: 0xd99b44 });
    const pocketGeo = new THREE.CircleGeometry(0.058, 24);
    const pocketMat = new THREE.MeshBasicMaterial({ color: 0x0b0b0b });

    for (const [px, py] of POCKET_COORDS) {
      const cap = new THREE.Mesh(capGeo, capMat);
      cap.position.set(px, py, 0.18);
      this.root.add(cap);

      const pocket = new THREE.Mesh(pocketGeo, pocketMat);
      pocket.position.set(px, py, 0.2);
      this.root.add(pocket);
    }
  }
}
