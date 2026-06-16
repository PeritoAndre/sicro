// @ts-nocheck -- vendored do arcada (Apache-2.0), vide planta/ATTRIBUTION.md; interfaceado via tipos do SICRO
import { PluginManager, Viewport } from "pixi-viewport";
import type { IViewportOptions } from "pixi-viewport";
import { Application, Graphics, InteractionEvent, isMobile, Loader, Point } from "pixi.js";
import { FloorPlan } from "./objects/FloorPlan";
import { TransformLayer } from "./objects/TransformControls/TransformLayer";
import { useStore } from "../../stores/EditorStore";
import { AddNodeAction } from "./actions/AddNodeAction";
import { AddWallManager } from "./actions/AddWallManager";
import { viewportX, viewportY } from "../../helpers/ViewportCoordinates";
import { METER, Tool } from "./constants";
import { Pointer } from "./Pointer";
import { Preview } from "./actions/MeasureToolManager";

export class Main extends Viewport {


    private floorPlan: FloorPlan;
    public static viewportPluginManager: PluginManager;
    public static app: Application;
    transformLayer: TransformLayer;
    addWallManager: AddWallManager;
    gridGraphics: Graphics;
    private gridRaf = 0;
    public pointer: Pointer;
    public preview: Preview;
    constructor(options: IViewportOptions) {
        super(options);

        // connect the events
        Loader.shared.onComplete.once(this.setup, this);
        // Start loading!
        Loader.shared.load();
        this.preview = new Preview();
        this.addChild(this.preview.getReference());
        this.cursor = "none";
    }

    private setup() {
        Main.viewportPluginManager = this.plugins;
        this.drag({ mouseButtons: 'right' }).clamp({ direction: 'all' })
            .pinch()
            .wheel().clampZoom({ minScale: 0.12, maxScale: 8.0 })
        this.center = new Point(this.worldWidth / 2, this.worldHeight / 2)

        // Grid VETORIAL com level-of-detail (LOD), em vez de TilingSprite — linhas
        // desenhadas conforme o zoom, com largura constante em px de tela. Elimina o
        // moiré que a textura repetida gerava em zooms intermediários. Inclui a folha
        // (#f5f6f8) e a borda da área útil (#525252).
        this.gridGraphics = new Graphics();
        this.addChild(this.gridGraphics);
        this.redrawGrid();

        this.floorPlan = FloorPlan.Instance;
        this.addChild(this.floorPlan);

        this.transformLayer = TransformLayer.Instance;
        this.addChild(this.transformLayer)

        this.addWallManager = AddWallManager.Instance;
        this.addChild(this.addWallManager.preview.getReference())

        this.pointer = new Pointer();
        this.addChild(this.pointer);
        this.on("pointerdown", this.checkTools)
        this.on("pointermove", this.updatePreview)
        this.on("pointerup", this.updateEnd)
        // Ao mudar zoom/posição: atualiza --viewport-zoom (escala da caixa de cota,
        // port arcada PR #14) e redesenha o grid vetorial (LOD).
        this.on("zoomed", this.onViewportChanged)
        this.on("moved", this.scheduleGridRedraw)
        this.onViewportChanged();
    }

    private onViewportChanged = () => {
        document.documentElement.style.setProperty("--viewport-zoom", this.scale.x.toString());
        this.scheduleGridRedraw();
    };

    /** Redesenha o grid no próximo frame (throttle — zoom/pan disparam muito). */
    private scheduleGridRedraw = () => {
        if (this.gridRaf) return;
        this.gridRaf = requestAnimationFrame(() => {
            this.gridRaf = 0;
            this.redrawGrid();
        });
    };

    /**
     * Grid vetorial com level-of-detail: desenha só a região visível, com larguras
     * de linha constantes em px de tela (÷ escala) e níveis (0,1 / 0,5 / 1 m) que
     * só aparecem quando têm espaço suficiente — sem moiré em nenhum zoom.
     */
    private redrawGrid() {
        const g = this.gridGraphics;
        if (!g) return;
        g.clear();

        const W = this.worldWidth ?? 0;
        const H = this.worldHeight ?? 0;
        const scale = this.scale.x || 1;
        const pxw = (px: number) => px / scale; // largura constante em px de tela

        // folha (área útil) sobre o backdrop cinza
        g.beginFill(0xf5f6f8);
        g.drawRect(0, 0, W, H);
        g.endFill();

        // região visível, limitada à folha — desenha só o que aparece
        let vb: any;
        try {
            vb = this.getVisibleBounds();
        } catch {
            vb = { x: 0, y: 0, width: W, height: H };
        }
        const x0 = Math.max(0, vb.x);
        const y0 = Math.max(0, vb.y);
        const x1 = Math.min(W, vb.x + vb.width);
        const y1 = Math.min(H, vb.y + vb.height);

        const lines = (step: number, color: number, widthPx: number) => {
            if (x1 <= x0 || y1 <= y0) return;
            g.lineStyle(pxw(widthPx), color, 1);
            for (let x = Math.ceil(x0 / step) * step; x <= x1; x += step) {
                g.moveTo(x, y0);
                g.lineTo(x, y1);
            }
            for (let y = Math.ceil(y0 / step) * step; y <= y1; y += step) {
                g.moveTo(x0, y);
                g.lineTo(x1, y);
            }
        };

        // LOD: cada nível só quando seu passo tiver largura suficiente em tela.
        if (0.1 * METER * scale >= 6) lines(0.1 * METER, 0xc4ccd9, 0.8); // 0,1 m (snap)
        if (0.5 * METER * scale >= 5) lines(0.5 * METER, 0x97a4b6, 1.0); // 0,5 m
        lines(1 * METER, 0x66748a, 1.4); // 1 m (sempre)

        // borda da folha (largura constante em tela)
        g.lineStyle(pxw(2.5), 0x525252, 1);
        g.drawRect(0, 0, W, H);
    }
    private updatePreview(ev: InteractionEvent) {
        this.addWallManager.updatePreview(ev);
        this.preview.updatePreview(ev);
        this.pointer.update(ev);
    }
    private updateEnd(ev: InteractionEvent) {
        switch (useStore.getState().activeTool) {
            case Tool.Measure:
                this.preview.set(undefined);
                this.pause = false;
                break;
            case Tool.WallAdd:
                if (!isMobile) {
                    this.pause = false;
                }
                break;
            case Tool.Edit:
                this.pause = false;
                break;
        }
    }
    private checkTools(ev: InteractionEvent) {
        ev.stopPropagation()
        if (ev.data.button == 2) {
            // Botão direito: cancela o desenho em andamento (parede/medição) — o
            // pan (right-drag) continua funcionando normalmente.
            const tool = useStore.getState().activeTool;
            if (tool === Tool.WallAdd) {
                this.addWallManager.cancelChain();
            } else if (tool === Tool.Measure) {
                this.preview.set(undefined);
            }
            return;
        }
        let point = { x: 0, y: 0 }
        switch (useStore.getState().activeTool) {
            case Tool.WallAdd:
                point.x = Math.max(0, Math.min(this.worldWidth, viewportX(ev.data.global.x)))
                point.y = Math.max(0, Math.min(this.worldHeight, viewportY(ev.data.global.y)));
                let action = new AddNodeAction(undefined, point)
                action.execute();
                break;
            case Tool.Edit:
                // if (!isMobile) {
                //     this.pause = true;
                // }
                break;
            case Tool.Measure:
                point.x = Math.max(0, Math.min(this.worldWidth, viewportX(ev.data.global.x)))
                point.y = Math.max(0, Math.min(this.worldHeight, viewportY(ev.data.global.y)));
                this.preview.set(point);
                break;
            case Tool.FurnitureAdd: {
                // Mobília LIVRE (SICRO): coloca a peça selecionada no ponto clicado.
                const def = useStore.getState().pendingFurniture;
                if (def) {
                    point.x = Math.max(0, Math.min(this.worldWidth, viewportX(ev.data.global.x)))
                    point.y = Math.max(0, Math.min(this.worldHeight, viewportY(ev.data.global.y)));
                    FloorPlan.Instance.addFurniture(def, undefined, point);
                }
                break;
            }
        }
    }

}
