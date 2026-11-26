import * as THREE from "three";

import type { CheckersView } from "../wasm_bridge.js";

export interface CheckersBoardCallbacks {
  readonly onSelectSquare: (square: number) => void;
  readonly onDragMove: (from: number, to: number) => void;
  readonly onClearSelection: () => void;
}

const PIECE_HEIGHT = 0.14;

function notationToRowCol(notation: number): readonly [number, number] {
  const idx = notation - 1;
  const row = Math.floor(idx / 4);
  const colInRow = idx % 4;
  const col = row % 2 === 0 ? colInRow * 2 + 1 : colInRow * 2;
  return [row, col];
}

function rowColToNotation(row: number, col: number): number | null {
  if (row < 0 || row >= 8 || col < 0 || col >= 8) {
    return null;
  }
  if ((row + col) % 2 === 0) {
    return null;
  }
  return row * 4 + Math.floor(col / 2) + 1;
}

function createNumberTexture(num: number): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    ctx.clearRect(0, 0, 64, 64);
    ctx.fillStyle = "rgba(255, 246, 224, 0.92)";
    ctx.font = "bold 24px Verdana, Tahoma, sans-serif";
    ctx.textAlign = "left";
    ctx.textBaseline = "top";
    ctx.fillText(String(num), 6, 4);
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

function createCrownTexture(): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 128;
  canvas.height = 128;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    ctx.clearRect(0, 0, 128, 128);
    ctx.fillStyle = "#f4c430";
    ctx.strokeStyle = "#4d3200";
    ctx.lineWidth = 5;
    ctx.beginPath();
    ctx.moveTo(26, 90);
    ctx.lineTo(22, 42);
    ctx.lineTo(48, 62);
    ctx.lineTo(64, 30);
    ctx.lineTo(80, 62);
    ctx.lineTo(106, 42);
    ctx.lineTo(102, 90);
    ctx.closePath();
    ctx.fill();
    ctx.stroke();
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

function createWoodSquareTexture(baseHex: string, grainHex: string): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 128;
  canvas.height = 128;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    ctx.fillStyle = baseHex;
    ctx.fillRect(0, 0, 128, 128);
    ctx.fillStyle = grainHex;
    for (let y = 0; y < 128; y += 6) {
      const alpha = ((y * 13) % 7) * 0.012 + 0.025;
      ctx.globalAlpha = alpha;
      ctx.fillRect(0, y, 128, 2);
    }
    ctx.globalAlpha = 1;
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.anisotropy = 4;
  return tex;
}

function createTabletopTexture(): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 256;
  canvas.height = 256;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    ctx.fillStyle = "#241912";
    ctx.fillRect(0, 0, 256, 256);
    for (let y = 0; y < 256; y += 32) {
      ctx.fillStyle = y % 64 === 0 ? "#2b1d15" : "#201610";
      ctx.fillRect(0, y, 256, 30);
      ctx.fillStyle = "#140d09";
      ctx.fillRect(0, y + 30, 256, 2);
    }
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.wrapS = THREE.RepeatWrapping;
  tex.wrapT = THREE.RepeatWrapping;
  tex.repeat.set(8, 8);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

function createBoardShadowTexture(): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 256;
  canvas.height = 256;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    const grad = ctx.createRadialGradient(128, 128, 52, 128, 128, 124);
    grad.addColorStop(0, "rgba(0, 0, 0, 0.72)");
    grad.addColorStop(0.7, "rgba(0, 0, 0, 0.38)");
    grad.addColorStop(1, "rgba(0, 0, 0, 0.0)");
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, 256, 256);
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

export class CheckersScene {
  public readonly root = new THREE.Group();

  private readonly numberPlanes: THREE.Mesh[] = [];
  private readonly lastMoveLayer = new THREE.Group();
  private readonly highlightLayer = new THREE.Group();
  private readonly pieceLayer = new THREE.Group();
  private readonly dragLayer = new THREE.Group();
  private readonly roomGroup = new THREE.Group();
  private readonly focusOutline: THREE.Mesh;

  // Lighting rig: strictly flat at blend=0 (classic 2D top-down look),
  // smoothly transitioning into warm parlor directional lighting in 3D fullscreen.
  private readonly ambientLight: THREE.AmbientLight;
  private readonly keyLight: THREE.DirectionalLight;
  private readonly fillLight: THREE.DirectionalLight;
  private readonly overheadLight: THREE.PointLight;

  // Shared 3D piece & board overlay geometries
  private readonly darkSquareGeo = new THREE.PlaneGeometry(1.0, 1.0);
  private readonly ringGeo = new THREE.RingGeometry(0.41, 0.48, 32);
  private readonly targetDotGeo = new THREE.CircleGeometry(0.18, 24);
  private readonly pieceBodyGeo: THREE.CylinderGeometry;
  private readonly pieceOuterRidgeGeo = new THREE.TorusGeometry(0.34, 0.032, 10, 32);
  private readonly pieceInnerRidgeGeo = new THREE.TorusGeometry(0.21, 0.018, 8, 28);
  private readonly kingCollarGeo = new THREE.TorusGeometry(0.385, 0.016, 8, 32);
  private readonly pieceShadowGeo = new THREE.CircleGeometry(0.44, 24);
  private readonly crownGeo = new THREE.PlaneGeometry(0.42, 0.42);

  // Shared materials
  private readonly crownMat: THREE.MeshBasicMaterial;
  private readonly blackOuterMat = new THREE.MeshStandardMaterial({
    color: 0x1e1e21,
    roughness: 0.34,
    metalness: 0.08,
  });
  private readonly blackInnerMat = new THREE.MeshStandardMaterial({
    color: 0x333338,
    roughness: 0.3,
    metalness: 0.1,
  });
  private readonly redOuterMat = new THREE.MeshStandardMaterial({
    color: 0x9e1b1b,
    roughness: 0.3,
    metalness: 0.08,
  });
  private readonly redInnerMat = new THREE.MeshStandardMaterial({
    color: 0xd13636,
    roughness: 0.28,
    metalness: 0.1,
  });
  private readonly goldTrimMat = new THREE.MeshStandardMaterial({
    color: 0xe6b850,
    roughness: 0.28,
    metalness: 0.6,
  });
  private readonly pieceShadowMat = new THREE.MeshBasicMaterial({
    color: 0x0c0704,
    transparent: true,
    opacity: 0.38,
    depthWrite: false,
  });

  private readonly lastMoveMat = new THREE.MeshBasicMaterial({
    color: 0xe6b800,
    transparent: true,
    opacity: 0.36,
  });
  private readonly selectableRingMat = new THREE.MeshBasicMaterial({
    color: 0xffd23f,
    side: THREE.DoubleSide,
  });
  private readonly selectedSquareMat = new THREE.MeshBasicMaterial({
    color: 0x38bdf8,
    transparent: true,
    opacity: 0.42,
  });
  private readonly targetDotMat = new THREE.MeshBasicMaterial({
    color: 0x3ddc84,
    side: THREE.DoubleSide,
  });
  private readonly pathDotMat = new THREE.MeshBasicMaterial({
    color: 0x38bdf8,
    side: THREE.DoubleSide,
  });

  private readonly callbacks: CheckersBoardCallbacks;
  private currentView: CheckersView | null = null;
  private dragStartSquare: number | null = null;
  private dragStartClientX = 0;
  private dragStartClientY = 0;
  private isDragging = false;
  private dragVisual: THREE.Group | null = null;
  private keyboardFocus: number | null = null;

  public constructor(callbacks: CheckersBoardCallbacks) {
    this.callbacks = callbacks;

    this.ambientLight = new THREE.AmbientLight(0xffffff, 3.1);
    this.keyLight = new THREE.DirectionalLight(0xfff4df, 0.0);
    this.keyLight.position.set(2.5, 1.5, 9.5);
    this.fillLight = new THREE.DirectionalLight(0xdbeafe, 0.0);
    this.fillLight.position.set(8.5, 9.5, 6.0);
    this.overheadLight = new THREE.PointLight(0xfff1d0, 0.0, 26.0, 1.1);
    this.overheadLight.position.set(4.0, 4.0, 6.5);
    this.root.add(this.ambientLight, this.keyLight, this.fillLight, this.overheadLight);

    this.pieceBodyGeo = new THREE.CylinderGeometry(0.39, 0.405, PIECE_HEIGHT, 32);
    this.pieceBodyGeo.rotateX(Math.PI / 2);

    this.crownMat = new THREE.MeshBasicMaterial({
      map: createCrownTexture(),
      transparent: true,
    });

    this.buildStaticBoard3D();
    this.buildRoomBackdrop();

    const focusMat = new THREE.MeshBasicMaterial({ color: 0x00e5ff, side: THREE.DoubleSide });
    this.focusOutline = new THREE.Mesh(new THREE.RingGeometry(0.45, 0.5, 4), focusMat);
    this.focusOutline.rotation.z = Math.PI / 4;
    this.focusOutline.visible = false;

    this.root.add(
      this.lastMoveLayer,
      this.highlightLayer,
      this.pieceLayer,
      this.dragLayer,
      this.focusOutline,
    );

    this.set3DVisualBlend(0);
  }

  /**
   * Smoothly blends between flat 2D top-down lighting (`blend = 0`) and rich 3D carved
   * wooden parlor lighting + tabletop backdrop (`blend = 1`) as the fullscreen camera tilts.
   */
  public set3DVisualBlend(rawBlend: number): void {
    const t = Math.max(0, Math.min(1, rawBlend));
    this.ambientLight.intensity = 3.1 - t * 2.05;
    this.keyLight.intensity = t * 1.95;
    this.fillLight.intensity = t * 0.65;
    this.overheadLight.intensity = t * 4.2;
    this.roomGroup.visible = t > 0.01;
  }

  public isDragActive(): boolean {
    return this.isDragging || this.dragStartSquare !== null;
  }

  public cancelDrag(): void {
    this.dragStartSquare = null;
    this.isDragging = false;
    this.clearDragVisual();
  }

  public sync(view: CheckersView): void {
    this.currentView = view;
    this.syncSquareNumbers(view.flipped, view.showSquareNumbers);
    this.rebuildHighlights(view);
    this.rebuildPieces(view);
    this.syncFocusOutline(view.flipped);
  }

  public handlePointerDown(
    worldX: number,
    worldY: number,
    clientX: number,
    clientY: number,
  ): void {
    if (this.currentView === null) {
      return;
    }
    const sq = this.worldToNotation(worldX, worldY, this.currentView.flipped);
    if (sq === null) {
      this.callbacks.onClearSelection();
      return;
    }

    this.dragStartSquare = sq;
    this.dragStartClientX = clientX;
    this.dragStartClientY = clientY;
    this.isDragging = false;
    this.clearDragVisual();
  }

  public handlePointerMove(
    worldX: number,
    worldY: number,
    clientX: number,
    clientY: number,
  ): void {
    if (this.dragStartSquare === null || this.currentView === null) {
      return;
    }
    const dx = clientX - this.dragStartClientX;
    const dy = clientY - this.dragStartClientY;
    if (!this.isDragging && Math.hypot(dx, dy) >= 6) {
      const canDrag =
        this.currentView.selectableSquares.includes(this.dragStartSquare) ||
        this.currentView.selectedPath[this.currentView.selectedPath.length - 1] ===
          this.dragStartSquare;
      if (canDrag) {
        const pieceChar = this.effectivePieceAt(this.currentView, this.dragStartSquare);
        if (pieceChar !== null && pieceChar !== ".") {
          this.isDragging = true;
          this.dragVisual = this.makePieceGroup(pieceChar, false);
          this.dragVisual.scale.set(1.08, 1.08, 1.08);
          this.dragVisual.position.set(worldX, worldY, 0.36);
          this.dragLayer.add(this.dragVisual);
        }
      }
    }
    if (this.isDragging && this.dragVisual !== null) {
      this.dragVisual.position.set(worldX, worldY, 0.36);
    }
  }

  public handlePointerUp(worldX: number, worldY: number): void {
    if (this.dragStartSquare === null || this.currentView === null) {
      return;
    }
    const fromSq = this.dragStartSquare;
    const wasDragging = this.isDragging;
    this.dragStartSquare = null;
    this.isDragging = false;
    this.clearDragVisual();

    const targetSq = this.worldToNotation(worldX, worldY, this.currentView.flipped);
    if (wasDragging && targetSq !== null && targetSq !== fromSq) {
      this.callbacks.onDragMove(fromSq, targetSq);
    } else if (targetSq !== null) {
      this.keyboardFocus = targetSq;
      this.syncFocusOutline(this.currentView.flipped);
      this.callbacks.onSelectSquare(targetSq);
    } else {
      this.callbacks.onClearSelection();
    }
  }

  public handleKeyDown(key: string): boolean {
    if (this.currentView === null) {
      return false;
    }
    if (key === "Escape") {
      this.callbacks.onClearSelection();
      return true;
    }
    if (key === "Enter" || key === " ") {
      if (this.keyboardFocus !== null) {
        this.callbacks.onSelectSquare(this.keyboardFocus);
      } else {
        const firstSelectable = this.currentView.selectableSquares[0];
        if (firstSelectable !== undefined) {
          this.keyboardFocus = firstSelectable;
          this.syncFocusOutline(this.currentView.flipped);
          this.callbacks.onSelectSquare(firstSelectable);
        }
      }
      return true;
    }

    const dir = this.currentView.flipped ? -1 : 1;
    let dRow = 0;
    let dCol = 0;
    if (key === "ArrowUp") {
      dRow = -dir;
    } else if (key === "ArrowDown") {
      dRow = dir;
    } else if (key === "ArrowLeft") {
      dCol = -dir;
    } else if (key === "ArrowRight") {
      dCol = dir;
    } else {
      return false;
    }

    const current = this.keyboardFocus ?? this.currentView.selectableSquares[0] ?? 11;
    const [r, c] = notationToRowCol(current);
    const nextRow = Math.max(0, Math.min(7, r + dRow));
    let nextCol = Math.max(0, Math.min(7, c + dCol));
    // Snap to nearest dark square on row/col change.
    if ((nextRow + nextCol) % 2 === 0) {
      if (dCol !== 0) {
        nextCol = Math.max(0, Math.min(7, nextCol + dCol));
      } else {
        nextCol = nextCol < 7 ? nextCol + 1 : nextCol - 1;
      }
    }
    const nextSq = rowColToNotation(nextRow, nextCol);
    if (nextSq !== null) {
      this.keyboardFocus = nextSq;
      this.syncFocusOutline(this.currentView.flipped);
    }
    return true;
  }

  private buildStaticBoard3D(): void {
    const outerWoodMat = new THREE.MeshStandardMaterial({
      color: 0x4b2a16,
      roughness: 0.42,
      metalness: 0.08,
    });
    const darkTrimMat = new THREE.MeshStandardMaterial({
      color: 0x2b170b,
      roughness: 0.48,
      metalness: 0.06,
    });
    const lightSqMat = new THREE.MeshStandardMaterial({
      map: createWoodSquareTexture("#edd6b0", "#c9a675"),
      roughness: 0.56,
      metalness: 0.02,
    });
    const darkSqMat = new THREE.MeshStandardMaterial({
      map: createWoodSquareTexture("#8c5a32", "#593518"),
      roughness: 0.54,
      metalness: 0.04,
    });

    // 1. Solid wooden board core beneath the 8x8 inlaid field (top face at z = -0.002)
    const boardCore = new THREE.Mesh(new THREE.BoxGeometry(8.56, 8.56, 0.24), outerWoodMat);
    boardCore.position.set(4.0, 4.0, -0.122);
    this.root.add(boardCore);

    // 2. Stepped lower carved walnut plinth (100% occluded from straight +Z, revealed in 3D!)
    const lowerPlinth = new THREE.Mesh(new THREE.BoxGeometry(8.88, 8.88, 0.16), darkTrimMat);
    lowerPlinth.position.set(4.0, 4.0, -0.26);
    this.root.add(lowerPlinth);

    // 3. Raised carved outer walnut frame rails (8.6 x 8.6 outer, 8.18 inner; z = -0.16 .. +0.055)
    const frameThick = 0.21;
    const frameHeight = 0.215;
    const frameZ = -0.0525;
    for (const y of [-0.09 - frameThick / 2, 8.09 + frameThick / 2]) {
      const rail = new THREE.Mesh(
        new THREE.BoxGeometry(8.6, frameThick, frameHeight),
        outerWoodMat,
      );
      rail.position.set(4.0, y, frameZ);
      this.root.add(rail);
    }
    for (const x of [-0.09 - frameThick / 2, 8.09 + frameThick / 2]) {
      const rail = new THREE.Mesh(
        new THREE.BoxGeometry(frameThick, 8.18, frameHeight),
        outerWoodMat,
      );
      rail.position.set(x, 4.0, frameZ);
      this.root.add(rail);
    }

    // 4. Inner dark ebony bevel border around the 8x8 playing field (8.18 outer, 8.0 inner)
    const bevelW = 0.09;
    const bevelH = 0.036;
    const bevelZ = 0.004; // spans z = -0.014 .. +0.022
    for (const y of [-bevelW / 2, 8.0 + bevelW / 2]) {
      const b = new THREE.Mesh(new THREE.BoxGeometry(8.18, bevelW, bevelH), darkTrimMat);
      b.position.set(4.0, y, bevelZ);
      this.root.add(b);
    }
    for (const x of [-bevelW / 2, 8.0 + bevelW / 2]) {
      const b = new THREE.Mesh(new THREE.BoxGeometry(bevelW, 8.0, bevelH), darkTrimMat);
      b.position.set(x, 4.0, bevelZ);
      this.root.add(b);
    }

    // 5. Decorative brass corner inlay diamonds flush on top of the walnut frame
    const cornerInlayGeo = new THREE.CircleGeometry(0.068, 4);
    const cornerCoords: readonly (readonly [number, number])[] = [
      [-0.195, -0.195],
      [8.195, -0.195],
      [8.195, 8.195],
      [-0.195, 8.195],
    ];
    for (const [cx, cy] of cornerCoords) {
      const inlay = new THREE.Mesh(cornerInlayGeo, this.goldTrimMat);
      inlay.position.set(cx, cy, 0.058);
      this.root.add(inlay);
    }

    // 6. 8x8 inlaid wooden squares (top surface at z = 0.0)
    for (let row = 0; row < 8; row += 1) {
      for (let col = 0; col < 8; col += 1) {
        const isDark = (row + col) % 2 === 1;
        const sq = new THREE.Mesh(this.darkSquareGeo, isDark ? darkSqMat : lightSqMat);
        sq.position.set(col + 0.5, 7.5 - row, 0.0);
        this.root.add(sq);
      }
    }

    // 7. Optional 1-32 square notation overlays
    const labelGeo = new THREE.PlaneGeometry(0.92, 0.92);
    for (let notation = 1; notation <= 32; notation += 1) {
      const mat = new THREE.MeshBasicMaterial({
        map: createNumberTexture(notation),
        transparent: true,
        depthWrite: false,
      });
      const mesh = new THREE.Mesh(labelGeo, mat);
      mesh.position.z = 0.012;
      mesh.visible = false;
      this.root.add(mesh);
      this.numberPlanes.push(mesh);
    }
  }

  private buildRoomBackdrop(): void {
    // Parlor wooden tabletop underneath the carved board cabinet (z = -0.35)
    const tabletop = new THREE.Mesh(
      new THREE.PlaneGeometry(26, 26),
      new THREE.MeshStandardMaterial({
        map: createTabletopTexture(),
        roughness: 0.74,
        metalness: 0.04,
      }),
    );
    tabletop.position.set(4.0, 4.0, -0.35);
    this.roomGroup.add(tabletop);

    const boardShadow = new THREE.Mesh(
      new THREE.PlaneGeometry(11.4, 11.4),
      new THREE.MeshBasicMaterial({
        map: createBoardShadowTexture(),
        transparent: true,
        depthWrite: false,
      }),
    );
    boardShadow.position.set(4.0, 4.0, -0.342);
    this.roomGroup.add(boardShadow);

    this.roomGroup.visible = false;
    this.root.add(this.roomGroup);
  }

  private notationToWorld(notation: number, flipped: boolean): readonly [number, number] {
    const [row, col] = notationToRowCol(notation);
    const drawRow = flipped ? 7 - row : row;
    const drawCol = flipped ? 7 - col : col;
    return [drawCol + 0.5, 7.5 - drawRow];
  }

  private worldToNotation(worldX: number, worldY: number, flipped: boolean): number | null {
    const drawCol = Math.floor(worldX);
    const drawRow = Math.floor(8.0 - worldY);
    if (drawRow < 0 || drawRow >= 8 || drawCol < 0 || drawCol >= 8) {
      return null;
    }
    const row = flipped ? 7 - drawRow : drawRow;
    const col = flipped ? 7 - drawCol : drawCol;
    return rowColToNotation(row, col);
  }

  private syncSquareNumbers(flipped: boolean, visible: boolean): void {
    for (let i = 0; i < 32; i += 1) {
      const plane = this.numberPlanes[i];
      if (plane === undefined) {
        continue;
      }
      plane.visible = visible;
      if (visible) {
        const [wx, wy] = this.notationToWorld(i + 1, flipped);
        plane.position.set(wx, wy, 0.012);
      }
    }
  }

  private rebuildHighlights(view: CheckersView): void {
    this.lastMoveLayer.clear();
    this.highlightLayer.clear();

    // Last move origin & destination
    if (view.state.lastMove !== null && view.state.lastMove.length >= 2) {
      const first = view.state.lastMove[0];
      const last = view.state.lastMove[view.state.lastMove.length - 1];
      for (const sq of [first, last]) {
        if (sq !== undefined) {
          const [wx, wy] = this.notationToWorld(sq, view.flipped);
          const m = new THREE.Mesh(this.darkSquareGeo, this.lastMoveMat);
          m.position.set(wx, wy, 0.006);
          this.lastMoveLayer.add(m);
        }
      }
    }

    // Mandatory capture rings around selectable pieces
    if (view.state.mandatoryCapture) {
      for (const sq of view.selectableSquares) {
        const [wx, wy] = this.notationToWorld(sq, view.flipped);
        const ring = new THREE.Mesh(this.ringGeo, this.selectableRingMat);
        ring.position.set(wx, wy, 0.025);
        this.highlightLayer.add(ring);
      }
    }

    // Selected square & intermediate multi-jump hops
    view.selectedPath.forEach((sq, idx) => {
      const [wx, wy] = this.notationToWorld(sq, view.flipped);
      if (idx === 0) {
        const sel = new THREE.Mesh(this.darkSquareGeo, this.selectedSquareMat);
        sel.position.set(wx, wy, 0.008);
        this.highlightLayer.add(sel);
      } else {
        const hop = new THREE.Mesh(this.targetDotGeo, this.pathDotMat);
        hop.position.set(wx, wy, 0.022);
        this.highlightLayer.add(hop);
      }
    });

    // Legal target squares
    for (const target of view.highlightedTargets) {
      const [wx, wy] = this.notationToWorld(target, view.flipped);
      const dot = new THREE.Mesh(this.targetDotGeo, this.targetDotMat);
      dot.position.set(wx, wy, 0.024);
      this.highlightLayer.add(dot);
    }
  }

  private rebuildPieces(view: CheckersView): void {
    this.pieceLayer.clear();
    const board = view.state.board;

    // If mid-multi-jump, visually preview the piece at the latest hop square.
    const jumpRoot = view.selectedPath.length > 1 ? (view.selectedPath[0] ?? null) : null;
    const jumpTip =
      view.selectedPath.length > 1
        ? (view.selectedPath[view.selectedPath.length - 1] ?? null)
        : null;

    for (let i = 0; i < 32; i += 1) {
      const sq = i + 1;
      let ch = board[i] ?? ".";
      if (jumpRoot !== null && sq === jumpRoot) {
        ch = ".";
      } else if (jumpRoot !== null && jumpTip !== null && sq === jumpTip) {
        ch = board[jumpRoot - 1] ?? ".";
      }
      if (ch === ".") {
        continue;
      }

      const piece = this.makePieceGroup(ch, true);
      const [wx, wy] = this.notationToWorld(sq, view.flipped);
      piece.position.set(wx, wy, 0.0);
      this.pieceLayer.add(piece);
    }
  }

  private effectivePieceAt(view: CheckersView, sq: number): string | null {
    const jumpRoot = view.selectedPath.length > 1 ? (view.selectedPath[0] ?? null) : null;
    const jumpTip =
      view.selectedPath.length > 1
        ? (view.selectedPath[view.selectedPath.length - 1] ?? null)
        : null;
    if (jumpRoot !== null && jumpTip !== null && sq === jumpTip) {
      return view.state.board[jumpRoot - 1] ?? null;
    }
    return view.state.board[sq - 1] ?? null;
  }

  private makePieceGroup(ch: string, includeShadow: boolean): THREE.Group {
    const group = new THREE.Group();
    const isBlack = ch === "b" || ch === "B";
    const isKing = ch === "B" || ch === "W";
    const outerMat = isBlack ? this.blackOuterMat : this.redOuterMat;
    const innerMat = isBlack ? this.blackInnerMat : this.redInnerMat;

    if (includeShadow) {
      const shadow = new THREE.Mesh(this.pieceShadowGeo, this.pieceShadowMat);
      shadow.position.z = 0.004;
      group.add(shadow);
    }

    // Bottom carved wooden disc (z = 0 .. PIECE_HEIGHT)
    const bottomDisc = new THREE.Mesh(this.pieceBodyGeo, outerMat);
    bottomDisc.position.z = PIECE_HEIGHT * 0.5;
    group.add(bottomDisc);

    if (isKing) {
      // Physically stacked double-height 3D King with brass collar ring & top crown medallion!
      const collar = new THREE.Mesh(this.kingCollarGeo, this.goldTrimMat);
      collar.position.z = PIECE_HEIGHT + 0.004;
      group.add(collar);

      const topDisc = new THREE.Mesh(this.pieceBodyGeo, outerMat);
      topDisc.position.z = PIECE_HEIGHT * 1.54;
      group.add(topDisc);

      const topZ = PIECE_HEIGHT * 2.04;
      const outerRidge = new THREE.Mesh(this.pieceOuterRidgeGeo, innerMat);
      outerRidge.position.z = topZ;
      const innerRidge = new THREE.Mesh(this.pieceInnerRidgeGeo, innerMat);
      innerRidge.position.z = topZ + 0.002;
      const crown = new THREE.Mesh(this.crownGeo, this.crownMat);
      crown.position.z = topZ + 0.025;
      group.add(outerRidge, innerRidge, crown);
    } else {
      // Lathe-turned concentric wooden ridges on top of single checker
      const outerRidge = new THREE.Mesh(this.pieceOuterRidgeGeo, innerMat);
      outerRidge.position.z = PIECE_HEIGHT;
      const innerRidge = new THREE.Mesh(this.pieceInnerRidgeGeo, innerMat);
      innerRidge.position.z = PIECE_HEIGHT + 0.002;
      group.add(outerRidge, innerRidge);
    }

    return group;
  }

  private syncFocusOutline(flipped: boolean): void {
    if (this.keyboardFocus === null) {
      this.focusOutline.visible = false;
      return;
    }
    const [wx, wy] = this.notationToWorld(this.keyboardFocus, flipped);
    this.focusOutline.position.set(wx, wy, 0.03);
    this.focusOutline.visible = true;
  }

  private clearDragVisual(): void {
    this.dragLayer.clear();
    this.dragVisual = null;
  }
}
