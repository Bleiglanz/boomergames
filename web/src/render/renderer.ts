import * as THREE from "three";

import type { GameKindDto } from "../wasm_bridge.js";

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
  private readonly checkersGroup: THREE.Group;
  private readonly ballMeshes: THREE.Group[] = [];
  private activeGame: GameKindDto | null = null;

  public constructor(container: HTMLElement) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: "default" });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.innerHTML = "";
    container.appendChild(this.renderer.domElement);

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x1f3a24);
    this.camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 10);
    this.camera.position.set(0, 0, 5);
    this.camera.lookAt(0, 0, 0);

    this.poolGroup = this.buildPoolStage();
    this.checkersGroup = this.buildCheckersStage();
    this.scene.add(this.poolGroup);
    this.scene.add(this.checkersGroup);

    this.resize(container);
  }

  public setGame(game: GameKindDto): void {
    this.activeGame = game;
    this.poolGroup.visible = game === "pool";
    this.checkersGroup.visible = game === "checkers";
    this.scene.background = new THREE.Color(game === "pool" ? 0x264129 : 0x3a2a1e);
  }

  public resize(container: HTMLElement): void {
    const width = Math.max(1, container.clientWidth);
    const height = Math.max(1, container.clientHeight);
    this.renderer.setSize(width, height, false);
    this.updateCameraProjection(width / height);
  }

  public render(poolFrame?: Float64Array): void {
    if (this.activeGame === "pool" && poolFrame !== undefined) {
      this.syncPoolBalls(poolFrame);
    }
    this.renderer.render(this.scene, this.camera);
  }

  public dispose(): void {
    this.renderer.dispose();
  }

  private updateCameraProjection(aspect: number): void {
    const worldW = this.activeGame === "checkers" ? 9.0 : TABLE_WIDTH + RAIL_MARGIN * 2.4;
    const worldH = this.activeGame === "checkers" ? 9.0 : TABLE_HEIGHT + RAIL_MARGIN * 2.4;
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

    this.camera.left = cx - halfW;
    this.camera.right = cx + halfW;
    this.camera.top = cy + halfH;
    this.camera.bottom = cy - halfH;
    this.camera.updateProjectionMatrix();
  }

  private buildPoolStage(): THREE.Group {
    const group = new THREE.Group();

    // Outer maroon wood rail frame (classic 2000s style)
    const railMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(TABLE_WIDTH + RAIL_MARGIN * 2, TABLE_HEIGHT + RAIL_MARGIN * 2),
      new THREE.MeshBasicMaterial({ color: 0x7c1717 }),
    );
    railMesh.position.set(TABLE_WIDTH / 2, TABLE_HEIGHT / 2, 0.0);
    group.add(railMesh);

    // Darker inner cushion edge
    const cushionMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(TABLE_WIDTH + 0.06, TABLE_HEIGHT + 0.06),
      new THREE.MeshBasicMaterial({ color: 0x1e661e }),
    );
    cushionMesh.position.set(TABLE_WIDTH / 2, TABLE_HEIGHT / 2, 0.05);
    group.add(cushionMesh);

    // Playing surface cloth (#2E8B2E)
    const clothMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(TABLE_WIDTH, TABLE_HEIGHT),
      new THREE.MeshBasicMaterial({ color: 0x2e8b2e }),
    );
    clothMesh.position.set(TABLE_WIDTH / 2, TABLE_HEIGHT / 2, 0.1);
    group.add(clothMesh);

    // Head string line
    const headLine = new THREE.Mesh(
      new THREE.PlaneGeometry(0.004, TABLE_HEIGHT),
      new THREE.MeshBasicMaterial({ color: 0x62b262 }),
    );
    headLine.position.set(0.635, TABLE_HEIGHT / 2, 0.15);
    group.add(headLine);

    // 6 solid black pockets with tan accent caps
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

    // 16 flat 2D pool balls
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

  private buildCheckersStage(): THREE.Group {
    const group = new THREE.Group();

    const frame = new THREE.Mesh(
      new THREE.PlaneGeometry(8.5, 8.5),
      new THREE.MeshBasicMaterial({ color: 0x5c361e }),
    );
    frame.position.set(4.0, 4.0, 0.0);
    group.add(frame);

    const sqGeo = new THREE.PlaneGeometry(1.0, 1.0);
    const lightMat = new THREE.MeshBasicMaterial({ color: 0xedd6b0 });
    const darkMat = new THREE.MeshBasicMaterial({ color: 0x8c5a32 });
    const discGeo = new THREE.CircleGeometry(0.38, 28);
    const darkDiscMat = new THREE.MeshBasicMaterial({ color: 0x1e1e1e });
    const redDiscMat = new THREE.MeshBasicMaterial({ color: 0xb82222 });

    for (let row = 0; row < 8; row += 1) {
      for (let col = 0; col < 8; col += 1) {
        const isDark = (row + col) % 2 === 1;
        const sq = new THREE.Mesh(sqGeo, isDark ? darkMat : lightMat);
        sq.position.set(col + 0.5, 7.5 - row, 0.1);
        group.add(sq);

        if (isDark && (row < 3 || row > 4)) {
          const piece = new THREE.Mesh(discGeo, row < 3 ? darkDiscMat : redDiscMat);
          piece.position.set(col + 0.5, 7.5 - row, 0.25);
          group.add(piece);
        }
      }
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
