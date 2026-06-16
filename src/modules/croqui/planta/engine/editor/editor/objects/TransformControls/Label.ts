// @ts-nocheck -- vendored do arcada (Apache-2.0), vide planta/ATTRIBUTION.md; interfaceado via tipos do SICRO
import { Container, Sprite, Text, TextStyle, Texture } from "pixi.js";
import type { Point } from "../../../../helpers/Point";
import { METER, Tool, WALL_THICKNESS } from "../../constants";
import { useStore } from "../../../../stores/EditorStore";
import { labelOffsetMap, wallStyleKey } from "../../../../wallStyles";

// Callback SICRO: avisa quando uma cota foi ARRASTADA, pra persistir o offset
// (em doc.labelOffsets) e entrar no Ctrl+Z. Registrado pelo PlantaEditor.
let labelMovedHandler: ((key: string, x: number, y: number) => void) | null =
  null;
export function setLabelMovedHandler(
  fn: ((key: string, x: number, y: number) => void) | null,
) {
  labelMovedHandler = fn;
}

export class Label extends Container {
    text:Text;
    textStyle:TextStyle = new TextStyle({fontFamily : 'Arial', fontSize: 16, fill : 0x000000, align : 'center'});
    textBkg :Sprite = new Sprite(Texture.WHITE); 
    constructor(sizeInPixels?: number) {
        super();
        if (!sizeInPixels) {
            sizeInPixels = 0;
        }
        this.text = new Text("", this.textStyle);
        this.update(sizeInPixels);

        this.addChild(this.textBkg);
        this.addChild(this.text);
        this.pivot.set(this.width / 2, this.height / 2);
        this.zIndex = 1001;

        this.on("toggleLabel", this.toggleLabel);
        this.toggleLabel({});

        // Editar a medida (clique) ou ARRASTAR a cota (drag), no modo Selecionar.
        this.interactive = true;
        this.cursor = "text";
        this.on("pointerdown", this.onLabelDown);
        this.on("pointermove", this.onLabelMove);
        this.on("pointerup", this.onLabelUp);
        this.on("pointerupoutside", this.onLabelUp);
        this.on("click", this.onClick);
    }

    private dragging = false;
    private wasDragged = false;
    private grabDX = 0;
    private grabDY = 0;

    private onLabelDown = (ev: any) => {
        ev.stopPropagation();
        if (useStore.getState().activeTool !== Tool.Edit) return;
        this.dragging = true;
        this.wasDragged = false;
        const p = ev.data.getLocalPosition(this.parent);
        this.grabDX = this.position.x - p.x;
        this.grabDY = this.position.y - p.y;
    };

    private onLabelMove = (ev: any) => {
        if (!this.dragging) return;
        const p = ev.data.getLocalPosition(this.parent);
        this.wasDragged = true;
        this.cursor = "move";
        this.position.set(p.x + this.grabDX, p.y + this.grabDY);
    };

    private onLabelUp = () => {
        if (!this.dragging) return;
        this.dragging = false;
        this.cursor = "text";
        if (!this.wasDragged) return;
        // Persiste o offset (coords locais da parede: base = meio da parede, 25).
        const parent: any = this.parent;
        if (!parent || !parent.leftNode || !parent.rightNode) return;
        const offX = this.position.x - (parent.width || 0) / 2;
        const offY = this.position.y - 25;
        const key = wallStyleKey(parent.leftNode.getId(), parent.rightNode.getId());
        labelOffsetMap.set(key, { x: offX, y: offY });
        if (labelMovedHandler) labelMovedHandler(key, offX, offY);
    };

    private lastClickTs = 0;
    private _dirty = false; // o valor da cota foi REALMENTE digitado?

    private onClick(ev: any) {
        // Se acabou de arrastar, não faz nada (foi mover a cota).
        if (this.wasDragged) {
            this.wasDragged = false;
            return;
        }
        if (useStore.getState().activeTool !== Tool.Edit) return;
        // Clique SIMPLES = só seleção (tratada pelo host SICRO). DUPLO-clique
        // abre o editor de medida. Isso evita que o clique de SELECIONAR a parede
        // — que cai sobre a cota, no meio dela — abra o editor e, num blur com
        // valor stale do input compartilhado, redimensione a parede sem querer.
        const now = performance.now();
        if (now - this.lastClickTs < 320) {
            this.lastClickTs = 0;
            ev.stopPropagation();
            this.openEditor();
        } else {
            this.lastClickTs = now;
        }
    }

    private openEditor() {
        const input = document.getElementById("label-input") as HTMLInputElement | null;
        if (!input) return;

        // No arcada o canvas ocupa a janela toda (coords ≈ página); no SICRO o
        // canvas tem offset (sidebar+cabeçalho), então somamos o rect do canvas.
        const canvas = document.getElementById("planta-pixi-canvas");
        const cr = canvas ? canvas.getBoundingClientRect() : { left: 0, top: 0 };
        const b = this.getBounds();
        input.style.pointerEvents = "auto";
        // O input (translate(-50%,-50%)) é centrado no CENTRO do rótulo — assim
        // o caret bate com os dígitos.
        input.style.top = `${cr.top + b.y + b.height / 2}px`;
        input.style.left = `${cr.left + b.x + b.width / 2}px`;

        // Esconde o rótulo Pixi enquanto edita (o campo visível fica por cima).
        this.text.visible = false;
        this._dirty = false;

        input.value = this.text.text.replace("m", "").replace(",", ".");
        input.focus();
        input.select();

        input.addEventListener("input", this._handleChangeInput);
        input.addEventListener("blur", this._handleBlurInput);
    }

    private _handleChangeInput = (ev: any) => {
        this._dirty = true;
        this.text.text = `${ev.target.value}m`;
        this.textBkg.width = this.text.width;
        this.textBkg.height = this.text.height;
    };

    private _handleBlurInput = (ev: any) => {
        const v = parseFloat(ev.target.value);
        // Só redimensiona se o valor foi REALMENTE alterado (digitação). Abrir e
        // fechar sem mexer NÃO altera a parede — mata o "selecionar encolhe".
        if (this._dirty && isFinite(v) && v > 0) {
            // a cota mostra o vão (comprimento − espessura) → soma de volta.
            this.parent?.updateWallLength?.(v * METER + WALL_THICKNESS);
        }
        this._dirty = false;
        this.text.visible = true; // volta o rótulo Pixi
        // Estaciona o campo FORA da tela ao fechar — agora ele é visível (antes
        // era transparente), então sem isso ficava aparecendo no lugar da cota.
        ev.target.style.pointerEvents = "none";
        ev.target.style.left = "-9999px";
        ev.target.style.top = "-9999px";
        ev.target.removeEventListener("input", this._handleChangeInput);
        ev.target.removeEventListener("blur", this._handleBlurInput);
    };

    private toggleLabel(ev:any) {
    }
    public update(sizeInPixels:number) {

        this.text.text = this.toMeter(sizeInPixels);
        this.textBkg.width = this.text.width;
        this.textBkg.height = this.text.height;


    }

    public updatePos(pos:Point, sizeInPixels:number) {
        this.position.set(pos.x, pos.y)
        this.update(sizeInPixels)
    }

    private toMeter(size:number) {
        size = Math.abs(size) / METER;

        // truncating to the 2nd decimal
        const sizeLabel = (Math.round((size) * 100) / 100).toFixed(2)

        return sizeLabel + "m"
    }    
}
