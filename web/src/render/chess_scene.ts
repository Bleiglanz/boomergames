import * as THREE from "three";

import type { ChessView } from "../wasm_bridge.js";

export interface ChessBoardCallbacks {
  readonly onSelectSquare: (square: number) => void;
  readonly onDragMove: (from: number, to: number) => void;
  readonly onClearSelection: () => void;
}

const FILES = ["a", "b", "c", "d", "e", "f", "g", "h"] as const;
const PIECE_KEYS = ["P", "N", "B", "R", "Q", "K", "p", "n", "b", "r", "q", "k"] as const;
type PieceKey = (typeof PIECE_KEYS)[number];

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

function createCoordLabelTexture(
  label: string,
  align: "bottom-right" | "top-left",
  darkInk: boolean,
): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    ctx.clearRect(0, 0, 64, 64);
    ctx.fillStyle = darkInk ? "rgba(74, 42, 18, 0.9)" : "rgba(248, 236, 212, 0.94)";
    ctx.font = "bold 20px Verdana, Tahoma, sans-serif";
    if (align === "top-left") {
      ctx.textAlign = "left";
      ctx.textBaseline = "top";
      ctx.fillText(label, 5, 4);
    } else {
      ctx.textAlign = "right";
      ctx.textBaseline = "bottom";
      ctx.fillText(label, 59, 60);
    }
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

/**
 * Draws crisp, platform-independent classic 2D Staunton chess pieces onto a 256x256 texture.
 */
function createPiece2DTexture(piece: PieceKey): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 256;
  canvas.height = 256;
  const ctx = canvas.getContext("2d");
  if (ctx !== null) {
    const isWhite = piece === piece.toUpperCase();
    const kind = piece.toLowerCase();

    const fillGrad = ctx.createLinearGradient(64, 30, 192, 224);
    if (isWhite) {
      fillGrad.addColorStop(0, "#fffdf6");
      fillGrad.addColorStop(0.6, "#f4e7c8");
      fillGrad.addColorStop(1, "#d8c39b");
    } else {
      fillGrad.addColorStop(0, "#4c423d");
      fillGrad.addColorStop(0.5, "#26201d");
      fillGrad.addColorStop(1, "#120f0d");
    }

    const strokeColor = isWhite ? "#1c1510" : "#0b0806";
    const detailColor = isWhite ? "#2b211a" : "#ded2bc";

    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    ctx.fillStyle = fillGrad;
    ctx.strokeStyle = strokeColor;
    ctx.lineWidth = 9;

    // Subtle ground contact shadow
    ctx.save();
    ctx.fillStyle = "rgba(0, 0, 0, 0.24)";
    ctx.beginPath();
    ctx.ellipse(128, 224, 72, 16, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();

    const drawBase = (y: number, halfW: number): void => {
      ctx.beginPath();
      ctx.moveTo(128 - halfW, y);
      ctx.lineTo(128 + halfW, y);
      ctx.quadraticCurveTo(128 + halfW - 6, y - 22, 128 + halfW - 18, y - 26);
      ctx.lineTo(128 - halfW + 18, y - 26);
      ctx.quadraticCurveTo(128 - halfW + 6, y - 22, 128 - halfW, y);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();
    };

    if (kind === "p") {
      // Pawn
      ctx.beginPath();
      ctx.moveTo(74, 214);
      ctx.quadraticCurveTo(86, 168, 106, 144);
      ctx.lineTo(150, 144);
      ctx.quadraticCurveTo(170, 168, 182, 214);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();

      ctx.beginPath();
      ctx.ellipse(128, 134, 34, 13, 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();

      ctx.beginPath();
      ctx.arc(128, 92, 34, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
      drawBase(220, 62);
    } else if (kind === "r") {
      // Rook
      ctx.beginPath();
      ctx.moveTo(74, 212);
      ctx.lineTo(86, 108);
      ctx.lineTo(170, 108);
      ctx.lineTo(182, 212);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();

      // Battlements
      ctx.beginPath();
      ctx.moveTo(72, 108);
      ctx.lineTo(72, 54);
      ctx.lineTo(98, 54);
      ctx.lineTo(98, 76);
      ctx.lineTo(116, 76);
      ctx.lineTo(116, 54);
      ctx.lineTo(140, 54);
      ctx.lineTo(140, 76);
      ctx.lineTo(158, 76);
      ctx.lineTo(158, 54);
      ctx.lineTo(184, 54);
      ctx.lineTo(184, 108);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();
      drawBase(220, 68);
    } else if (kind === "n") {
      // Knight
      ctx.beginPath();
      ctx.moveTo(74, 212);
      ctx.quadraticCurveTo(78, 162, 102, 132);
      ctx.quadraticCurveTo(82, 128, 66, 134);
      ctx.quadraticCurveTo(54, 118, 66, 98);
      ctx.quadraticCurveTo(84, 72, 112, 54);
      ctx.lineTo(116, 36);
      ctx.lineTo(138, 52);
      ctx.quadraticCurveTo(180, 62, 186, 116);
      ctx.quadraticCurveTo(192, 164, 182, 212);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();

      // Eye & mane accents
      ctx.fillStyle = detailColor;
      ctx.beginPath();
      ctx.arc(105, 88, 6, 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = detailColor;
      ctx.lineWidth = 6;
      ctx.beginPath();
      ctx.moveTo(148, 74);
      ctx.quadraticCurveTo(166, 108, 158, 162);
      ctx.stroke();

      ctx.fillStyle = fillGrad;
      ctx.strokeStyle = strokeColor;
      ctx.lineWidth = 9;
      drawBase(220, 68);
    } else if (kind === "b") {
      // Bishop
      ctx.beginPath();
      ctx.moveTo(76, 212);
      ctx.quadraticCurveTo(86, 164, 104, 140);
      ctx.quadraticCurveTo(82, 102, 128, 52);
      ctx.quadraticCurveTo(174, 102, 152, 140);
      ctx.quadraticCurveTo(170, 164, 180, 212);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();

      // Finial ball
      ctx.beginPath();
      ctx.arc(128, 42, 14, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();

      // Mitre diagonal slash + collar
      ctx.strokeStyle = detailColor;
      ctx.lineWidth = 7;
      ctx.beginPath();
      ctx.moveTo(118, 92);
      ctx.lineTo(146, 118);
      ctx.moveTo(100, 144);
      ctx.lineTo(156, 144);
      ctx.stroke();

      ctx.strokeStyle = strokeColor;
      ctx.lineWidth = 9;
      drawBase(220, 66);
    } else if (kind === "q") {
      // Queen
      ctx.beginPath();
      ctx.moveTo(72, 210);
      ctx.lineTo(52, 82);
      ctx.lineTo(92, 126);
      ctx.lineTo(100, 64);
      ctx.lineTo(128, 116);
      ctx.lineTo(156, 64);
      ctx.lineTo(164, 126);
      ctx.lineTo(204, 82);
      ctx.lineTo(184, 210);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();

      // Crown pearls
      const pearls: readonly (readonly [number, number])[] = [
        [52, 76],
        [100, 56],
        [128, 48],
        [156, 56],
        [204, 76],
      ];
      for (const [px, py] of pearls) {
        ctx.beginPath();
        ctx.arc(px, py, 10, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
      }
      drawBase(220, 70);
    } else {
      // King
      ctx.beginPath();
      ctx.moveTo(74, 210);
      ctx.quadraticCurveTo(54, 140, 82, 108);
      ctx.quadraticCurveTo(104, 86, 128, 106);
      ctx.quadraticCurveTo(152, 86, 174, 108);
      ctx.quadraticCurveTo(202, 140, 182, 210);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();

      // Cross atop crown
      ctx.lineWidth = 10;
      ctx.beginPath();
      ctx.moveTo(128, 96);
      ctx.lineTo(128, 36);
      ctx.moveTo(104, 58);
      ctx.lineTo(152, 58);
      ctx.stroke();

      // Inner crown line
      ctx.strokeStyle = detailColor;
      ctx.lineWidth = 6;
      ctx.beginPath();
      ctx.moveTo(128, 108);
      ctx.lineTo(128, 186);
      ctx.stroke();

      ctx.strokeStyle = strokeColor;
      ctx.lineWidth = 9;
      drawBase(220, 70);
    }

    // Crisp rim accent for dark pieces so they pop cleanly against dark walnut squares
    if (!isWhite) {
      ctx.strokeStyle = "rgba(242, 226, 194, 0.42)";
      ctx.lineWidth = 3;
      ctx.beginPath();
      ctx.moveTo(72, 214);
      ctx.lineTo(184, 214);
      ctx.stroke();
    }
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.anisotropy = 4;
  return tex;
}

function makeLatheZ(profile: readonly (readonly [number, number])[], segments = 24): THREE.LatheGeometry {
  const pts = profile.map(([r, z]) => new THREE.Vector2(r, z));
  const geo = new THREE.LatheGeometry(pts, segments);
  // Three.js LatheGeometry revolves around +Y; rotate so +Y maps to +Z (vertical up off board)
  geo.rotateX(Math.PI / 2);
  geo.computeVertexNormals();
  return geo;
}

function createKnightHeadGeometry(): THREE.ExtrudeGeometry {
  const shape = new THREE.Shape();
  shape.moveTo(-0.16, 0.0);
  shape.lineTo(0.18, 0.0);
  shape.quadraticCurveTo(0.22, 0.22, 0.14, 0.44);
  shape.quadraticCurveTo(0.08, 0.56, -0.02, 0.62);
  shape.lineTo(-0.08, 0.54);
  shape.quadraticCurveTo(-0.22, 0.44, -0.26, 0.3);
  shape.quadraticCurveTo(-0.24, 0.22, -0.1, 0.24);
  shape.quadraticCurveTo(-0.08, 0.14, -0.16, 0.0);
  const geo = new THREE.ExtrudeGeometry(shape, {
    depth: 0.18,
    bevelEnabled: true,
    bevelThickness: 0.035,
    bevelSize: 0.03,
    bevelSegments: 3,
  });
  geo.translate(0, 0, -0.09);
  // Orient shape upright along +Z
  geo.rotateX(Math.PI / 2);
  geo.computeVertexNormals();
  return geo;
}

export class ChessScene {
  public readonly root = new THREE.Group();

  private readonly coordLayer = new THREE.Group();
  private readonly lastMoveLayer = new THREE.Group();
  private readonly highlightLayer = new THREE.Group();
  private readonly pieceLayer = new THREE.Group();
  private readonly dragLayer = new THREE.Group();
  private readonly roomGroup = new THREE.Group();
  private readonly focusOutline: THREE.Mesh;

  // Lighting rig matching Checkers' smooth 2D -> 3D parlor transition
  private readonly ambientLight: THREE.AmbientLight;
  private readonly keyLight: THREE.DirectionalLight;
  private readonly fillLight: THREE.DirectionalLight;
  private readonly overheadLight: THREE.PointLight;

  // Shared board & overlay geometries
  private readonly squareGeo = new THREE.PlaneGeometry(1.0, 1.0);
  private readonly piece2DGeo = new THREE.PlaneGeometry(0.92, 0.92);
  private readonly targetDotGeo = new THREE.CircleGeometry(0.16, 24);
  private readonly captureRingGeo = new THREE.RingGeometry(0.38, 0.47, 32);
  private readonly pieceShadowGeo = new THREE.CircleGeometry(0.39, 24);

  // Shared sculpted 3D Staunton geometries
  private readonly pawnGeo: THREE.LatheGeometry;
  private readonly rookGeo: THREE.LatheGeometry;
  private readonly bishopGeo: THREE.LatheGeometry;
  private readonly queenGeo: THREE.LatheGeometry;
  private readonly kingGeo: THREE.LatheGeometry;
  private readonly knightBaseGeo: THREE.LatheGeometry;
  private readonly knightHeadGeo: THREE.ExtrudeGeometry;
  private readonly crossVGeo = new THREE.BoxGeometry(0.055, 0.055, 0.19);
  private readonly crossHGeo = new THREE.BoxGeometry(0.16, 0.055, 0.055);
  private readonly merlonGeo = new THREE.BoxGeometry(0.11, 0.11, 0.085);
  private readonly baseCollarGeo = new THREE.TorusGeometry(0.33, 0.018, 8, 28);

  // Materials
  private readonly piece2DMats = new Map<PieceKey, THREE.MeshBasicMaterial>();
  private readonly whiteWoodMat = new THREE.MeshStandardMaterial({
    color: 0xf4e8ce,
    roughness: 0.32,
    metalness: 0.06,
  });
  private readonly blackWoodMat = new THREE.MeshStandardMaterial({
    color: 0x261d19,
    roughness: 0.34,
    metalness: 0.12,
  });
  private readonly goldTrimMat = new THREE.MeshStandardMaterial({
    color: 0xe6b850,
    roughness: 0.28,
    metalness: 0.6,
  });
  private readonly pieceShadowMat = new THREE.MeshBasicMaterial({
    color: 0x0c0704,
    transparent: true,
    opacity: 0.34,
    depthWrite: false,
  });
  private readonly lastMoveMat = new THREE.MeshBasicMaterial({
    color: 0xe6b800,
    transparent: true,
    opacity: 0.38,
  });
  private readonly checkSquareMat = new THREE.MeshBasicMaterial({
    color: 0xdc2626,
    transparent: true,
    opacity: 0.56,
  });
  private readonly selectedSquareMat = new THREE.MeshBasicMaterial({
    color: 0x38bdf8,
    transparent: true,
    opacity: 0.44,
  });
  private readonly targetDotMat = new THREE.MeshBasicMaterial({
    color: 0x2ec774,
    transparent: true,
    opacity: 0.85,
    side: THREE.DoubleSide,
  });
  private readonly captureRingMat = new THREE.MeshBasicMaterial({
    color: 0x2ec774,
    transparent: true,
    opacity: 0.9,
    side: THREE.DoubleSide,
  });

  private readonly callbacks: ChessBoardCallbacks;
  private currentView: ChessView | null = null;
  private currentBlend = 0;
  private dragStartSquare: number | null = null;
  private dragStartClientX = 0;
  private dragStartClientY = 0;
  private isDragging = false;
  private dragVisual: THREE.Group | null = null;
  private keyboardFocus: number | null = null;

  public constructor(callbacks: ChessBoardCallbacks) {
    this.callbacks = callbacks;

    this.ambientLight = new THREE.AmbientLight(0xffffff, 3.1);
    this.keyLight = new THREE.DirectionalLight(0xfff4df, 0.0);
    this.keyLight.position.set(2.5, 1.5, 9.5);
    this.fillLight = new THREE.DirectionalLight(0xdbeafe, 0.0);
    this.fillLight.position.set(8.5, 9.5, 6.0);
    this.overheadLight = new THREE.PointLight(0xfff1d0, 0.0, 26.0, 1.1);
    this.overheadLight.position.set(4.0, 4.0, 6.5);
    this.root.add(this.ambientLight, this.keyLight, this.fillLight, this.overheadLight);

    // Build shared 2D Staunton textures
    for (const key of PIECE_KEYS) {
      this.piece2DMats.set(
        key,
        new THREE.MeshBasicMaterial({
          map: createPiece2DTexture(key),
          transparent: true,
          opacity: 1.0,
          depthWrite: false,
        }),
      );
    }

    // Build shared 3D Staunton lathe geometries
    this.pawnGeo = makeLatheZ([
      [0.0, 0.0],
      [0.31, 0.0],
      [0.31, 0.08],
      [0.24, 0.14],
      [0.16, 0.34],
      [0.12, 0.44],
      [0.19, 0.48],
      [0.12, 0.52],
      [0.18, 0.62],
      [0.16, 0.72],
      [0.08, 0.78],
      [0.0, 0.79],
    ]);
    this.rookGeo = makeLatheZ([
      [0.0, 0.0],
      [0.34, 0.0],
      [0.34, 0.09],
      [0.26, 0.16],
      [0.21, 0.56],
      [0.26, 0.64],
      [0.26, 0.76],
      [0.18, 0.76],
      [0.18, 0.7],
      [0.0, 0.7],
    ]);
    this.bishopGeo = makeLatheZ([
      [0.0, 0.0],
      [0.33, 0.0],
      [0.33, 0.09],
      [0.25, 0.16],
      [0.16, 0.48],
      [0.22, 0.54],
      [0.16, 0.58],
      [0.22, 0.72],
      [0.17, 0.88],
      [0.07, 0.96],
      [0.1, 1.01],
      [0.0, 1.04],
    ]);
    this.queenGeo = makeLatheZ([
      [0.0, 0.0],
      [0.36, 0.0],
      [0.36, 0.1],
      [0.28, 0.18],
      [0.17, 0.62],
      [0.25, 0.68],
      [0.18, 0.74],
      [0.27, 0.96],
      [0.2, 1.02],
      [0.12, 1.07],
      [0.09, 1.13],
      [0.0, 1.16],
    ]);
    this.kingGeo = makeLatheZ([
      [0.0, 0.0],
      [0.37, 0.0],
      [0.37, 0.1],
      [0.29, 0.18],
      [0.18, 0.66],
      [0.26, 0.73],
      [0.19, 0.79],
      [0.26, 1.02],
      [0.19, 1.12],
      [0.08, 1.17],
      [0.0, 1.18],
    ]);
    this.knightBaseGeo = makeLatheZ([
      [0.0, 0.0],
      [0.34, 0.0],
      [0.34, 0.09],
      [0.26, 0.16],
      [0.22, 0.26],
      [0.0, 0.26],
    ]);
    this.knightHeadGeo = createKnightHeadGeometry();

    this.buildStaticBoard3D();
    this.buildRoomBackdrop();

    const focusMat = new THREE.MeshBasicMaterial({ color: 0x00e5ff, side: THREE.DoubleSide });
    this.focusOutline = new THREE.Mesh(new THREE.RingGeometry(0.45, 0.5, 4), focusMat);
    this.focusOutline.rotation.z = Math.PI / 4;
    this.focusOutline.visible = false;

    this.root.add(
      this.coordLayer,
      this.lastMoveLayer,
      this.highlightLayer,
      this.pieceLayer,
      this.dragLayer,
      this.focusOutline,
    );

    this.set3DVisualBlend(0);
  }

  /**
   * Smoothly blends between classic crisp 2D Staunton top-down view (`blend = 0`)
   * and sculpted 3D carved wooden Staunton pieces on a parlor tabletop (`blend = 1`).
   */
  public set3DVisualBlend(rawBlend: number): void {
    const t = Math.max(0, Math.min(1, rawBlend));
    this.currentBlend = t;

    this.ambientLight.intensity = 3.1 - t * 2.05;
    this.keyLight.intensity = t * 2.05;
    this.fillLight.intensity = t * 0.72;
    this.overheadLight.intensity = t * 4.4;
    this.roomGroup.visible = t > 0.01;

    // Crossfade 2D sprites out as sculpted 3D pieces rise up
    const opacity2D = Math.max(0, Math.min(1, 1 - (t - 0.12) / 0.42));
    for (const mat of this.piece2DMats.values()) {
      mat.opacity = opacity2D;
    }

    const show3D = t > 0.04;
    const scale3DZ = Math.max(0.06, Math.min(1, (t - 0.04) / 0.52));
    this.applyBlendToLayer(this.pieceLayer, opacity2D > 0.02, show3D, scale3DZ);
    this.applyBlendToLayer(this.dragLayer, opacity2D > 0.02, show3D, scale3DZ);
  }

  public isDragActive(): boolean {
    return this.isDragging || this.dragStartSquare !== null;
  }

  public cancelDrag(): void {
    this.dragStartSquare = null;
    this.isDragging = false;
    this.clearDragVisual();
  }

  public sync(view: ChessView): void {
    this.currentView = view;
    this.rebuildCoords(view.flipped, view.showCoords);
    this.rebuildHighlights(view);
    this.rebuildPieces(view);
    this.syncFocusOutline(view.flipped);
    this.set3DVisualBlend(this.currentBlend);
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
    const sq = this.worldToSquare(worldX, worldY, this.currentView.flipped);
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
      if (this.currentView.selectableSquares.includes(this.dragStartSquare)) {
        const pieceChar = this.pieceAtSquare(this.currentView.state.board, this.dragStartSquare);
        if (pieceChar !== null) {
          this.isDragging = true;
          this.dragVisual = this.makePieceGroup(pieceChar, false, this.currentView.flipped);
          this.dragVisual.scale.set(1.08, 1.08, 1.08);
          this.dragVisual.position.set(worldX, worldY, 0.34);
          this.dragLayer.add(this.dragVisual);
          this.set3DVisualBlend(this.currentBlend);
        }
      }
    }
    if (this.isDragging && this.dragVisual !== null) {
      this.dragVisual.position.set(worldX, worldY, 0.34);
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

    const targetSq = this.worldToSquare(worldX, worldY, this.currentView.flipped);
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
    let dRank = 0;
    let dFile = 0;
    if (key === "ArrowUp") {
      dRank = dir;
    } else if (key === "ArrowDown") {
      dRank = -dir;
    } else if (key === "ArrowLeft") {
      dFile = -dir;
    } else if (key === "ArrowRight") {
      dFile = dir;
    } else {
      return false;
    }

    const current = this.keyboardFocus ?? this.currentView.selectableSquares[0] ?? 12; // e2
    const rank = Math.floor(current / 8);
    const file = current % 8;
    const nextRank = Math.max(0, Math.min(7, rank + dRank));
    const nextFile = Math.max(0, Math.min(7, file + dFile));
    this.keyboardFocus = nextRank * 8 + nextFile;
    this.syncFocusOutline(this.currentView.flipped);
    return true;
  }

  private applyBlendToLayer(
    layer: THREE.Group,
    show2D: boolean,
    show3D: boolean,
    scale3DZ: number,
  ): void {
    for (const child of layer.children) {
      const sprite2d = child.getObjectByName("sprite2d");
      if (sprite2d !== undefined) {
        sprite2d.visible = show2D;
      }
      const mesh3d = child.getObjectByName("mesh3d");
      if (mesh3d !== undefined) {
        mesh3d.visible = show3D;
        mesh3d.scale.set(1, 1, scale3DZ);
      }
    }
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
      map: createWoodSquareTexture("#f0d9b5", "#cfa978"),
      roughness: 0.56,
      metalness: 0.02,
    });
    const darkSqMat = new THREE.MeshStandardMaterial({
      map: createWoodSquareTexture("#9b673c", "#6c4221"),
      roughness: 0.54,
      metalness: 0.04,
    });

    // Solid board core beneath the 8x8 inlaid field
    const boardCore = new THREE.Mesh(new THREE.BoxGeometry(8.56, 8.56, 0.24), outerWoodMat);
    boardCore.position.set(4.0, 4.0, -0.122);
    this.root.add(boardCore);

    // Lower carved walnut plinth (revealed when tilting in 3D!)
    const lowerPlinth = new THREE.Mesh(new THREE.BoxGeometry(8.88, 8.88, 0.16), darkTrimMat);
    lowerPlinth.position.set(4.0, 4.0, -0.26);
    this.root.add(lowerPlinth);

    // Raised outer walnut frame rails
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

    // Inner dark ebony bevel border
    const bevelW = 0.09;
    const bevelH = 0.036;
    const bevelZ = 0.004;
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

    // Decorative brass corner inlay diamonds
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

    // 8x8 inlaid squares (a1 = (0.5, 0.5) is dark, h1 = (7.5, 0.5) is light)
    for (let rank = 0; rank < 8; rank += 1) {
      for (let file = 0; file < 8; file += 1) {
        const isDark = (rank + file) % 2 === 0;
        const sq = new THREE.Mesh(this.squareGeo, isDark ? darkSqMat : lightSqMat);
        sq.position.set(file + 0.5, rank + 0.5, 0.0);
        this.root.add(sq);
      }
    }
  }

  private buildRoomBackdrop(): void {
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

  private squareToWorld(sq: number, flipped: boolean): readonly [number, number] {
    const rank = Math.floor(sq / 8);
    const file = sq % 8;
    const drawFile = flipped ? 7 - file : file;
    const drawRank = flipped ? 7 - rank : rank;
    return [drawFile + 0.5, drawRank + 0.5];
  }

  private worldToSquare(worldX: number, worldY: number, flipped: boolean): number | null {
    const drawFile = Math.floor(worldX);
    const drawRank = Math.floor(worldY);
    if (drawFile < 0 || drawFile >= 8 || drawRank < 0 || drawRank >= 8) {
      return null;
    }
    const file = flipped ? 7 - drawFile : drawFile;
    const rank = flipped ? 7 - drawRank : drawRank;
    return rank * 8 + file;
  }

  private pieceAtSquare(board: string, sq: number): PieceKey | null {
    const rank = Math.floor(sq / 8);
    const file = sq % 8;
    const ch = board[(7 - rank) * 8 + file] ?? ".";
    if ((PIECE_KEYS as readonly string[]).includes(ch)) {
      return ch as PieceKey;
    }
    return null;
  }

  private rebuildCoords(flipped: boolean, visible: boolean): void {
    this.coordLayer.clear();
    if (!visible) {
      return;
    }
    const labelGeo = new THREE.PlaneGeometry(0.94, 0.94);
    for (let drawFile = 0; drawFile < 8; drawFile += 1) {
      const fileIdx = flipped ? 7 - drawFile : drawFile;
      const label = FILES[fileIdx] ?? "a";
      const isDarkSquare = drawFile % 2 === 0;
      const mat = new THREE.MeshBasicMaterial({
        map: createCoordLabelTexture(label, "bottom-right", !isDarkSquare),
        transparent: true,
        depthWrite: false,
      });
      const mesh = new THREE.Mesh(labelGeo, mat);
      mesh.position.set(drawFile + 0.5, 0.5, 0.011);
      this.coordLayer.add(mesh);
    }
    for (let drawRank = 0; drawRank < 8; drawRank += 1) {
      const rankNum = flipped ? 8 - drawRank : drawRank + 1;
      const isDarkSquare = drawRank % 2 === 0;
      const mat = new THREE.MeshBasicMaterial({
        map: createCoordLabelTexture(String(rankNum), "top-left", !isDarkSquare),
        transparent: true,
        depthWrite: false,
      });
      const mesh = new THREE.Mesh(labelGeo, mat);
      mesh.position.set(0.5, drawRank + 0.5, 0.011);
      this.coordLayer.add(mesh);
    }
  }

  private rebuildHighlights(view: ChessView): void {
    this.lastMoveLayer.clear();
    this.highlightLayer.clear();

    // Last move highlight
    if (view.state.lastMove !== null) {
      for (const sq of view.state.lastMove) {
        const [wx, wy] = this.squareToWorld(sq, view.flipped);
        const m = new THREE.Mesh(this.squareGeo, this.lastMoveMat);
        m.position.set(wx, wy, 0.006);
        this.lastMoveLayer.add(m);
      }
    }

    // King in check highlight
    if (view.state.checkSquare !== null) {
      const [wx, wy] = this.squareToWorld(view.state.checkSquare, view.flipped);
      const m = new THREE.Mesh(this.squareGeo, this.checkSquareMat);
      m.position.set(wx, wy, 0.009);
      this.highlightLayer.add(m);
    }

    // Selected square highlight
    if (view.selectedSquare !== null) {
      const [wx, wy] = this.squareToWorld(view.selectedSquare, view.flipped);
      const sel = new THREE.Mesh(this.squareGeo, this.selectedSquareMat);
      sel.position.set(wx, wy, 0.008);
      this.highlightLayer.add(sel);
    }

    // Legal move targets (dot for quiet moves, ring for captures)
    for (const target of view.highlightedTargets) {
      const [wx, wy] = this.squareToWorld(target, view.flipped);
      const isCapture =
        this.pieceAtSquare(view.state.board, target) !== null ||
        view.state.legalMoves.some(
          (mv) => mv.from === view.selectedSquare && mv.to === target && mv.isCapture,
        );
      const marker = new THREE.Mesh(
        isCapture ? this.captureRingGeo : this.targetDotGeo,
        isCapture ? this.captureRingMat : this.targetDotMat,
      );
      marker.position.set(wx, wy, 0.022);
      this.highlightLayer.add(marker);
    }
  }

  private rebuildPieces(view: ChessView): void {
    this.pieceLayer.clear();
    for (let sq = 0; sq < 64; sq += 1) {
      const pieceChar = this.pieceAtSquare(view.state.board, sq);
      if (pieceChar === null) {
        continue;
      }
      const piece = this.makePieceGroup(pieceChar, true, view.flipped);
      const [wx, wy] = this.squareToWorld(sq, view.flipped);
      piece.position.set(wx, wy, 0.0);
      this.pieceLayer.add(piece);
    }
  }

  private makePieceGroup(piece: PieceKey, includeShadow: boolean, flipped: boolean): THREE.Group {
    const group = new THREE.Group();
    const isWhite = piece === piece.toUpperCase();
    const kind = piece.toLowerCase();
    const woodMat = isWhite ? this.whiteWoodMat : this.blackWoodMat;

    if (includeShadow) {
      const shadow = new THREE.Mesh(this.pieceShadowGeo, this.pieceShadowMat);
      shadow.position.z = 0.005;
      group.add(shadow);
    }

    // 1. Crisp 2D top-down Staunton sprite (shown at blend = 0)
    const mat2D = this.piece2DMats.get(piece);
    if (mat2D !== undefined) {
      const sprite2d = new THREE.Mesh(this.piece2DGeo, mat2D);
      sprite2d.name = "sprite2d";
      sprite2d.position.z = 0.028;
      group.add(sprite2d);
    }

    // 2. Sculpted 3D lathe-turned Staunton wooden piece (rises cleanly when tilted in 3D!)
    const mesh3d = new THREE.Group();
    mesh3d.name = "mesh3d";

    const collar = new THREE.Mesh(this.baseCollarGeo, this.goldTrimMat);
    collar.position.z = 0.022;
    mesh3d.add(collar);

    if (kind === "p") {
      mesh3d.add(new THREE.Mesh(this.pawnGeo, woodMat));
    } else if (kind === "r") {
      mesh3d.add(new THREE.Mesh(this.rookGeo, woodMat));
      for (let i = 0; i < 6; i += 1) {
        const ang = (i * Math.PI) / 3;
        const merlon = new THREE.Mesh(this.merlonGeo, woodMat);
        merlon.position.set(Math.cos(ang) * 0.21, Math.sin(ang) * 0.21, 0.78);
        merlon.rotation.z = ang;
        mesh3d.add(merlon);
      }
    } else if (kind === "n") {
      mesh3d.add(new THREE.Mesh(this.knightBaseGeo, woodMat));
      const head = new THREE.Mesh(this.knightHeadGeo, woodMat);
      head.position.z = 0.22;
      // Face knight forward toward the opponent
      const forwardRot = isWhite === !flipped ? -Math.PI / 2 : Math.PI / 2;
      head.rotation.z = forwardRot;
      mesh3d.add(head);
    } else if (kind === "b") {
      mesh3d.add(new THREE.Mesh(this.bishopGeo, woodMat));
    } else if (kind === "q") {
      mesh3d.add(new THREE.Mesh(this.queenGeo, woodMat));
      const crownRing = new THREE.Mesh(new THREE.TorusGeometry(0.22, 0.024, 8, 24), this.goldTrimMat);
      crownRing.position.z = 0.96;
      mesh3d.add(crownRing);
    } else {
      mesh3d.add(new THREE.Mesh(this.kingGeo, woodMat));
      const vBar = new THREE.Mesh(this.crossVGeo, this.goldTrimMat);
      vBar.position.z = 1.26;
      const hBar = new THREE.Mesh(this.crossHGeo, this.goldTrimMat);
      hBar.position.z = 1.29;
      mesh3d.add(vBar, hBar);
    }

    mesh3d.visible = false;
    group.add(mesh3d);
    return group;
  }

  private syncFocusOutline(flipped: boolean): void {
    if (this.keyboardFocus === null) {
      this.focusOutline.visible = false;
      return;
    }
    const [wx, wy] = this.squareToWorld(this.keyboardFocus, flipped);
    this.focusOutline.position.set(wx, wy, 0.032);
    this.focusOutline.visible = true;
  }

  private clearDragVisual(): void {
    this.dragLayer.clear();
    this.dragVisual = null;
  }
}
