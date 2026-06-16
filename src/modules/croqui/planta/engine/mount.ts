// @ts-nocheck -- ponte com o motor Pixi vendido (arcada); tipos isolados aqui.
/**
 * Montagem/desmontagem do motor de planta (Pixi + Main/Viewport do arcada) e
 * ponte de persistência (FloorPlan.save/load). Mantido fora do PlantaEditor
 * (que é tipado) pra concentrar aqui o atrito de tipos do Pixi 6 + motor vendido.
 */
import { Application } from "pixi.js";
import { Main } from "./editor/editor/Main";
import { METER } from "./editor/editor/constants";
import { FloorPlan } from "./editor/editor/objects/FloorPlan";
import { TransformLayer } from "./editor/editor/objects/TransformControls/TransformLayer";
import { AddWallManager } from "./editor/editor/actions/AddWallManager";
import { DeleteWallAction } from "./editor/editor/actions/DeleteWallAction";
import { wallStyleMap, wallStyleKey, labelOffsetMap } from "./wallStyles";
export { setLabelMovedHandler } from "./editor/editor/objects/TransformControls/Label";
import { main, setMain } from "./editor/EditorRoot";
import { disposePlantaEngine } from "./reset";
import { getEvidenceLayerBoundsWorld } from "./evidenceLayer";

/** Cria a Application Pixi + o viewport Main dentro do `host`. */
export function mountPlanta(host) {
  disposePlantaEngine(); // começa limpo (singletons de mount anterior)

  const app = new Application({
    resolution: window.devicePixelRatio || 1,
    autoDensity: true,
    backgroundColor: 0xa3a3a3, // backdrop cinza (padrão do croqui viário)
    antialias: true,
    resizeTo: host,
  });
  app.view.oncontextmenu = (e) => e.preventDefault();
  app.view.id = "planta-pixi-canvas"; // o Label usa pra posicionar o input da cota
  host.appendChild(app.view);

  // Falha ALTO em vez de entregar `undefined` ao pixi-viewport (que viraria
  // um canvas em branco silencioso): se o contexto WebGL não inicializou no
  // WebView2, o plugin de interação não existe.
  const interaction = app.renderer.plugins?.interaction;
  if (!interaction) {
    app.destroy(true, true);
    throw new Error(
      "Pixi: plugin de interação indisponível (contexto WebGL provavelmente falhou no WebView2).",
    );
  }

  const main = new Main({
    screenWidth: app.screen.width,
    screenHeight: app.screen.height,
    worldWidth: 60 * METER,
    worldHeight: 60 * METER,
    interaction,
  });
  setMain(main); // expõe pro motor (Floor/ViewportCoordinates)
  app.stage.addChild(main);

  // Pixi 6 `resizeTo` só escuta o 'resize' do window, NÃO mudanças de tamanho
  // do elemento. Quando o layout flex assenta (painel mais largo que no mount),
  // o canvas ficava pequeno e a área restante do host não era canvas (logo,
  // não-editável). Observamos o host e redimensionamos renderer + viewport.
  const resizeBoth = () => {
    const w = host.clientWidth;
    const h = host.clientHeight;
    if (w > 0 && h > 0) {
      app.renderer.resize(w, h);
      main.resize(w, h, 60 * METER, 60 * METER);
      main.scheduleGridRedraw?.(); // a região visível mudou → redesenha o grid LOD
    }
  };
  const ro = new ResizeObserver(resizeBoth);
  ro.observe(host);
  resizeBoth(); // alinha já no primeiro frame
  app.__sicroResizeObserver = ro;
  return app;
}

/** Carrega o floorplan salvo (modelo do motor). Vazio → mantém o piso default. */
export function loadFloorplan(floorplan) {
  if (
    floorplan &&
    Array.isArray(floorplan.floors) &&
    floorplan.floors.length > 0
  ) {
    FloorPlan.Instance.load(JSON.stringify(floorplan));
  } else {
    // toca o singleton pra garantir o piso 0 default
    void FloorPlan.Instance;
  }
}

/** Serializa o floorplan atual (objeto JSON, pronto pro .sicroplanta). */
export function saveFloorplan() {
  return JSON.parse(FloorPlan.Instance.save());
}

/** Snapshot do floorplan como STRING (pro histórico de undo/redo). */
export function floorplanSnapshot() {
  try {
    return FloorPlan.Instance.save();
  } catch {
    return null;
  }
}

/** Restaura um snapshot (string) do floorplan — usado por undo/redo. */
export function restoreFloorplan(snapshot) {
  if (!snapshot) return;
  try {
    FloorPlan.Instance.load(snapshot);
    // O load recria os objetos; a TransformLayer (caixa de seleção) ainda
    // apontava pro objeto antigo, deixando os handles "presos". Desseleciona.
    TransformLayer.Instance?.deselect?.();
    // Zera a sequência de parede em andamento — o previousNode apontaria pra um
    // nó recriado (referência velha) e geraria parede fantasma no próximo clique.
    AddWallManager.Instance?.unset?.();
  } catch {
    /* snapshot inválido — ignora */
  }
}

/**
 * true se há uma parede em andamento com só o 1º clique (nó solto, sem parede).
 * O histórico de undo NÃO deve registrar esse estado transitório.
 */
export function wallChainPending() {
  try {
    return !!AddWallManager.Instance?.isPendingLoneNode?.();
  } catch {
    return false;
  }
}

/** Cancela a parede em andamento, removendo o nó solto do 1º clique (igual ao botão direito). */
export function cancelWallChain() {
  try {
    AddWallManager.Instance?.cancelChain?.();
  } catch {
    /* noop */
  }
}

// ---------------------------------------------------------------------------
// Estilo ("skin") por parede: muro / cerca / calçada. Persistido pelo SICRO em
// doc.wallStyles (mapa por par-de-nós). O Wall.drawLine lê o wallStyleMap.

function distSeg(px, py, x1, y1, x2, y2) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len2 = dx * dx + dy * dy;
  if (len2 === 0) return Math.hypot(px - x1, py - y1);
  let t = ((px - x1) * dx + (py - y1) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
}

/** Popula o mapa de estilos a partir do doc e redesenha as paredes. */
export function loadWallStyles(obj) {
  wallStyleMap.clear();
  if (obj && typeof obj === "object") {
    for (const k of Object.keys(obj)) wallStyleMap.set(k, obj[k]);
  }
  try {
    FloorPlan.Instance.redrawWalls();
  } catch {
    /* noop */
  }
}

/** Define (ou limpa, se "parede") o estilo de uma parede e re-renderiza. */
export function setWallStyle(key, kind) {
  if (!kind || kind === "parede") wallStyleMap.delete(key);
  else wallStyleMap.set(key, kind);
  try {
    FloorPlan.Instance.redrawWalls();
  } catch {
    /* noop */
  }
}

/**
 * Mostra/esconde TODOS os nós de parede (os "pontos pretos"). Eles são handles
 * de edição (ligação/rotação): visíveis só ao editar; escondidos no Navegar e no
 * export. visible=false também desliga o hit-test — OK, pois só escondemos em
 * modos read-only.
 */
export function setNodesVisible(visible) {
  try {
    const nodes = FloorPlan.Instance.getWallNodeSeq().getWallNodes();
    for (const node of nodes.values()) node.visible = visible;
  } catch {
    /* noop */
  }
}

/** Popula os offsets de cota a partir do doc e redesenha as paredes. */
export function loadLabelOffsets(obj) {
  labelOffsetMap.clear();
  if (obj && typeof obj === "object") {
    for (const k of Object.keys(obj)) {
      const v = obj[k];
      if (v && typeof v.x === "number" && typeof v.y === "number") {
        labelOffsetMap.set(k, { x: v.x, y: v.y });
      }
    }
  }
  try {
    FloorPlan.Instance.redrawWalls();
  } catch {
    /* noop */
  }
}

/** Mostra SÓ os nós cujos ids estão na lista (handles da parede selecionada). */
export function showOnlyNodes(ids) {
  try {
    const set = new Set(ids || []);
    const nodes = FloorPlan.Instance.getWallNodeSeq().getWallNodes();
    for (const node of nodes.values()) node.visible = set.has(node.getId());
  } catch {
    /* noop */
  }
}

/** Remove a parede do par de nós (Delete na parede selecionada). true se removeu. */
export function deleteWallByNodes(leftId, rightId) {
  try {
    const walls = FloorPlan.Instance.getWallNodeSeq().getWalls();
    const w = walls.find((x) => {
      const a = x.leftNode.getId();
      const b = x.rightNode.getId();
      return (a === leftId && b === rightId) || (a === rightId && b === leftId);
    });
    if (w) {
      new DeleteWallAction(w).execute();
      return true;
    }
  } catch {
    /* noop */
  }
  return false;
}

/** Acha a parede mais próxima do ponto (mundo) → { key, kind, lengthM } ou null. */
export function pickWallAt(wx, wy) {
  try {
    const walls = FloorPlan.Instance.getWallNodeSeq().getWalls();
    let best = null;
    let bestD = Infinity;
    for (const w of walls) {
      const d = distSeg(wx, wy, w.leftNode.x, w.leftNode.y, w.rightNode.x, w.rightNode.y);
      const thr = Math.max((w.thickness || 10) / 2, 22);
      if (d <= thr && d < bestD) {
        bestD = d;
        best = w;
      }
    }
    if (!best) return null;
    const leftId = best.leftNode.getId();
    const rightId = best.rightNode.getId();
    const key = wallStyleKey(leftId, rightId);
    return {
      key,
      kind: wallStyleMap.get(key) || "parede",
      lengthM: (best.length || 0) / METER,
      nodeIds: [leftId, rightId],
    };
  } catch {
    return null;
  }
}

/**
 * Troca de andar/piso. `by` = +1 sobe (cria o piso se não existir), -1 desce.
 * Descer abaixo do térreo (0) é ignorado. O índice do piso atual é refletido
 * em useStore().floor pelo próprio FloorPlan.
 */
export function changeFloor(by) {
  const fp = FloorPlan.Instance;
  if (by < 0 && (fp.CurrentFloor ?? 0) <= 0) return;
  fp.changeFloor(by);
}

/**
 * Captura a planta como PNG (data URL) enquadrando o CONTEÚDO desenhado
 * (paredes/mobília/vestígios) com margem, fundo branco, independente do
 * zoom/pan atual (restaura a vista ao final). Retorna também `imgPxPerM`
 * (px/metro na imagem) pra barra de escala exata na prancha.
 */
export function capturePlantaDataUrl(app) {
  if (!main || !app) return null;

  // bounds do conteúdo (mundo): floorplan (paredes/mobília) ∪ vestígios.
  const fp = FloorPlan.Instance;
  let fpB = null;
  try {
    const b = fp.getLocalBounds();
    if (b && b.width > 1 && b.height > 1) fpB = b;
  } catch {
    /* noop */
  }
  const evB = getEvidenceLayerBoundsWorld();

  let minX, minY, maxX, maxY;
  if (fpB || evB) {
    minX = Math.min(fpB ? fpB.x : Infinity, evB ? evB.x : Infinity);
    minY = Math.min(fpB ? fpB.y : Infinity, evB ? evB.y : Infinity);
    maxX = Math.max(fpB ? fpB.x + fpB.width : -Infinity, evB ? evB.x + evB.width : -Infinity);
    maxY = Math.max(fpB ? fpB.y + fpB.height : -Infinity, evB ? evB.y + evB.height : -Infinity);
  } else {
    // nada desenhado → região padrão de 20 m ao redor do centro do mundo
    const c = (main.worldWidth ?? 6000) / 2;
    minX = c - 1000;
    minY = c - 1000;
    maxX = c + 1000;
    maxY = c + 1000;
  }

  const margin = 120; // ~1,2 m
  const W = maxX - minX + margin * 2;
  const H = maxY - minY + margin * 2;
  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;

  const screen = app.renderer.screen;
  let s = Math.min(screen.width / W, screen.height / H);
  if (!isFinite(s) || s <= 0) s = 1;
  s = Math.max(0.05, Math.min(s, 4));

  // salva vista + fundo
  const savedScaleX = main.scale.x;
  const savedScaleY = main.scale.y;
  const savedPosX = main.position.x;
  const savedPosY = main.position.y;
  const savedBg = app.renderer.backgroundColor;
  // Esconde os nós (handles) na prancha — não devem sair no PNG técnico.
  let savedNodes = [];
  try {
    savedNodes = Array.from(
      FloorPlan.Instance.getWallNodeSeq().getWallNodes().values(),
    );
  } catch {
    /* noop */
  }
  const savedNodeVis = savedNodes.map((n) => n.visible);

  try {
    savedNodes.forEach((n) => (n.visible = false));
    main.scale.set(s, s); // bypassa clampZoom (só atua em wheel/setZoom)
    main.moveCenter(cx, cy);
    app.renderer.backgroundColor = 0xffffff; // margens brancas no export
    app.renderer.render(app.stage);
    const dataUrl = app.view.toDataURL("image/png");
    const imgPxPerM = METER * s * (app.renderer.resolution || 1);
    return { dataUrl, imgPxPerM };
  } catch {
    return null;
  } finally {
    // restaura a vista do perito
    savedNodes.forEach((n, i) => (n.visible = savedNodeVis[i] ?? true));
    app.renderer.backgroundColor = savedBg;
    main.scale.set(savedScaleX, savedScaleY);
    main.position.set(savedPosX, savedPosY);
    try {
      app.renderer.render(app.stage);
    } catch {
      /* noop */
    }
  }
}

/** Zoom multiplicativo mantendo o centro (clampado por clampZoom no Main). */
export function zoomBy(factor) {
  if (!main) return;
  main.pause = false; // garante viewport ativo (despausa qualquer estado preso)
  main.setZoom(main.scale.x * factor, true);
}

/** Zoom in / out / 100% — wrappers usados pelos botões da UI. */
export const zoomIn = () => zoomBy(1.25);
export const zoomOut = () => zoomBy(0.8);
export function zoomReset() {
  if (!main) return;
  main.pause = false;
  main.setZoom(1, true);
}

/** Destrói a Application e zera os singletons. */
export function destroyPlanta(app) {
  try {
    app?.__sicroResizeObserver?.disconnect();
  } catch {
    /* noop */
  }
  try {
    app?.destroy(true, true);
  } catch {
    /* noop */
  }
  disposePlantaEngine();
}
