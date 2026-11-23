import * as THREE from "three";

import type { CheckersView } from "../wasm_bridge.js";

export interface CheckersBoardCallbacks {
  readonly onSelectSquare: (square: number) => void;
  readonly onDragMove: (from: number, to: number) => void;
  readonly onClearSelection: () => void;
}

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
    ctx.fillStyle = "rgba(255, 246, 224, 0.9)";
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

export class CheckersScene {
  public readonly root = new THREE.Group();

  private readonly numberPlanes: THREE.Mesh[] = [];
  private readonly lastMoveLayer = new THREE.Group();
  private readonly highlightLayer = new THREE.Group();
  private readonly pieceLayer = new THREE.Group();
  private readonly dragLayer = new THREE.Group();
  private readonly focusOutline: THREE.Mesh;

  private readonly darkSquareGeo = new THREE.PlaneGeometry(1.0, 1.0);
  private readonly ringGeo = new THREE.RingGeometry(0.39, 0.46, 32);
  private readonly targetDotGeo = new THREE.CircleGeometry(0.18, 24);
  private readonly outerDiscGeo = new THREE.CircleGeometry(0.39, 32);
  private readonly innerDiscGeo = new THREE.CircleGeometry(0.31, 32);
  private readonly crownGeo = new THREE.PlaneGeometry(0.42, 0.42);

  private readonly crownMat: THREE.MeshBasicMaterial;
  private readonly blackOuterMat = new THREE.MeshBasicMaterial({ color: 0x181818 });
  private readonly blackInnerMat = new THREE.MeshBasicMaterial({ color: 0x333333 });
  private readonly redOuterMat = new THREE.MeshBasicMaterial({ color: 0x9e1b1b });
  private readonly redInnerMat = new THREE.MeshBasicMaterial({ color: 0xd13636 });

  private readonly lastMoveMat = new THREE.MeshBasicMaterial({
    color: 0xe6b800,
    transparent: true,
    opacity: 0.36,
  });
  private readonly selectableRingMat = new THREE.MeshBasicMaterial({ color: 0xffd23f });
  private readonly selectedSquareMat = new THREE.MeshBasicMaterial({
    color: 0x38bdf8,
    transparent: true,
    opacity: 0.42,
  });
  private readonly targetDotMat = new THREE.MeshBasicMaterial({ color: 0x3ddc84 });
  private readonly pathDotMat = new THREE.MeshBasicMaterial({ color: 0x38bdf8 });

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
    this.crownMat = new THREE.MeshBasicMaterial({
      map: createCrownTexture(),
      transparent: true,
    });

    this.buildStaticBoard();

    const focusMat = new THREE.MeshBasicMaterial({ color: 0x00e5ff });
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
          this.dragVisual = this.makePieceGroup(pieceChar);
          this.dragVisual.scale.set(1.12, 1.12, 1);
          this.dragVisual.position.set(worldX, worldY, 0.85);
          this.dragLayer.add(this.dragVisual);
        }
      }
    }
    if (this.isDragging && this.dragVisual !== null) {
      this.dragVisual.position.set(worldX, worldY, 0.85);
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
    let nextRow = Math.max(0, Math.min(7, r + dRow));
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

  private buildStaticBoard(): void {
    const outerFrame = new THREE.Mesh(
      new THREE.PlaneGeometry(8.6, 8.6),
      new THREE.MeshBasicMaterial({ color: 0x4b2a16 }),
    );
    outerFrame.position.set(4.0, 4.0, 0.0);
    this.root.add(outerFrame);

    const innerBevel = new THREE.Mesh(
      new THREE.PlaneGeometry(8.18, 8.18),
      new THREE.MeshBasicMaterial({ color: 0x2b170b }),
    );
    innerBevel.position.set(4.0, 4.0, 0.04);
    this.root.add(innerBevel);

    const lightMat = new THREE.MeshBasicMaterial({ color: 0xedd6b0 });
    const darkMat = new THREE.MeshBasicMaterial({ color: 0x8c5a32 });

    for (let row = 0; row < 8; row += 1) {
      for (let col = 0; col < 8; col += 1) {
        const isDark = (row + col) % 2 === 1;
        const sq = new THREE.Mesh(this.darkSquareGeo, isDark ? darkMat : lightMat);
        sq.position.set(col + 0.5, 7.5 - row, 0.1);
        this.root.add(sq);
      }
    }

    const labelGeo = new THREE.PlaneGeometry(0.92, 0.92);
    for (let notation = 1; notation <= 32; notation += 1) {
      const mat = new THREE.MeshBasicMaterial({
        map: createNumberTexture(notation),
        transparent: true,
      });
      const mesh = new THREE.Mesh(labelGeo, mat);
      mesh.position.z = 0.22;
      mesh.visible = false;
      this.root.add(mesh);
      this.numberPlanes.push(mesh);
    }
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
        plane.position.set(wx, wy, 0.22);
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
          m.position.set(wx, wy, 0.16);
          this.lastMoveLayer.add(m);
        }
      }
    }

    // Mandatory capture rings on selectable pieces
    if (view.state.mandatoryCapture) {
      for (const sq of view.selectableSquares) {
        const [wx, wy] = this.notationToWorld(sq, view.flipped);
        const ring = new THREE.Mesh(this.ringGeo, this.selectableRingMat);
        ring.position.set(wx, wy, 0.32);
        this.highlightLayer.add(ring);
      }
    }

    // Selected square & intermediate multi-jump hops
    view.selectedPath.forEach((sq, idx) => {
      const [wx, wy] = this.notationToWorld(sq, view.flipped);
      if (idx === 0) {
        const sel = new THREE.Mesh(this.darkSquareGeo, this.selectedSquareMat);
        sel.position.set(wx, wy, 0.18);
        this.highlightLayer.add(sel);
      } else {
        const hop = new THREE.Mesh(this.targetDotGeo, this.pathDotMat);
        hop.position.set(wx, wy, 0.34);
        this.highlightLayer.add(hop);
      }
    });

    // Legal target squares
    for (const target of view.highlightedTargets) {
      const [wx, wy] = this.notationToWorld(target, view.flipped);
      const dot = new THREE.Mesh(this.targetDotGeo, this.targetDotMat);
      dot.position.set(wx, wy, 0.35);
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

      const piece = this.makePieceGroup(ch);
      const [wx, wy] = this.notationToWorld(sq, view.flipped);
      piece.position.set(wx, wy, 0.45);
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

  private makePieceGroup(ch: string): THREE.Group {
    const group = new THREE.Group();
    const isBlack = ch === "b" || ch === "B";
    const isKing = ch === "B" || ch === "W";

    const outer = new THREE.Mesh(
      this.outerDiscGeo,
      isBlack ? this.blackOuterMat : this.redOuterMat,
    );
    const inner = new THREE.Mesh(
      this.innerDiscGeo,
      isBlack ? this.blackInnerMat : this.redInnerMat,
    );
    inner.position.z = 0.02;
    group.add(outer, inner);

    if (isKing) {
      const crown = new THREE.Mesh(this.crownGeo, this.crownMat);
      crown.position.z = 0.05;
      group.add(crown);
    }
    return group;
  }

  private syncFocusOutline(flipped: boolean): void {
    if (this.keyboardFocus === null) {
      this.focusOutline.visible = false;
      return;
    }
    const [wx, wy] = this.notationToWorld(this.keyboardFocus, flipped);
    this.focusOutline.position.set(wx, wy, 0.7);
    this.focusOutline.visible = true;
  }

  private clearDragVisual(): void {
    this.dragLayer.clear();
    this.dragVisual = null;
  }
}
