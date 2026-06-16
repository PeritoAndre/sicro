// @ts-nocheck -- vendored do arcada (Apache-2.0), vide planta/ATTRIBUTION.md; interfaceado via tipos do SICRO
import { Graphics, InteractionEvent, Rectangle } from "pixi.js";
import { getDoor, getWindow } from "../../../../api/api-client";
import { euclideanDistance } from "../../../../helpers/EuclideanDistance";
import type { Point } from "../../../../helpers/Point";
import { getCorrespondingY } from "../../../../helpers/Slope";
import { viewportX, viewportY } from "../../../../helpers/ViewportCoordinates";

import { useStore } from "../../../../stores/EditorStore";
import { AddFurnitureAction } from "../../actions/AddFurnitureAction";
import { AddNodeAction } from "../../actions/AddNodeAction";
import { DeleteWallAction } from "../../actions/DeleteWallAction";
import { INTERIOR_WALL_THICKNESS, Tool, WALL_THICKNESS } from "../../constants";
import { Label } from "../TransformControls/Label";
import { WallNode } from "./WallNode";
import { FloorPlan } from "../FloorPlan";
import { wallStyleMap, wallStyleKey, labelOffsetMap } from "../../../../wallStyles";

export class Wall extends Graphics {

    leftNode: WallNode;
    rightNode: WallNode;
    length: number;
    label: Label;

    x1: number;
    x2: number;
    y1: number;
    y2: number;
    thickness: number;
    isExteriorWall: boolean;

    dragging: boolean;
    mouseStartPoint: Point;
    startLeftNode: Point;
    startRightNode: Point;

    constructor(leftNode: WallNode, rightNode: WallNode) {
        super();
        this.sortableChildren = true;

        this.interactive = true;
        this.leftNode = leftNode;
        this.rightNode = rightNode;
        this.dragging = false;
        this.mouseStartPoint = {x:0, y:0}
        this.startLeftNode = {x:0, y:0}
        this.startRightNode = {x:0, y:0}
        this.setLineCoords();
        this.label = new Label(0);

        this.addChild(this.label)
        this.thickness = INTERIOR_WALL_THICKNESS;
        this.pivot.set(0, INTERIOR_WALL_THICKNESS / 2);
        this.zIndex = 100;
        this.isExteriorWall = false;
        // this.drawLine();


        this.on("pointerdown", this.onMouseDown)
        this.on("rightdown", this.onRightDown)
        this.on("pointermove", this.onMouseMove)
        this.on("pointerup", this.onMouseUp)
        this.on("pointerupoutside", this.onMouseUp);

    }

    public setIsExterior(value: boolean) {
        this.isExteriorWall = value;
        if (value) {
            this.thickness = WALL_THICKNESS;
        } else {
            this.thickness = INTERIOR_WALL_THICKNESS;
        }
        this.pivot.set(0, this.thickness / 2);
        this.leftNode.setSize(this.thickness);
        this.rightNode.setSize(this.thickness);
        this.drawLine();
    }

    public getIsExterior() {
        return this.isExteriorWall;
    }
    public setLineCoords() {
        if (this.leftNode.x == this.rightNode.x) {
            if (this.leftNode.y < this.rightNode.y) {
                return [this.leftNode.x, this.leftNode.y, this.rightNode.x, this.rightNode.y]
            } else {
                return [this.rightNode.x, this.rightNode.y, this.leftNode.x, this.leftNode.y]
            }
        } else if (this.leftNode.x < this.rightNode.x) {
            return [this.leftNode.x, this.leftNode.y, this.rightNode.x, this.rightNode.y]
        } else {
            return [this.rightNode.x, this.rightNode.y, this.leftNode.x, this.leftNode.y]
        }
    }

    public drawLine() {
        this.clear();
        [this.x1, this.y1, this.x2, this.y2] = this.setLineCoords();
        this.lineStyle(1, 0x1a1a1a);

        let theta = Math.atan2((this.y2 - this.y1), (this.x2 - this.x1)); // aflu unghiul sa pot roti
        theta *= 180 / Math.PI; // rads to degs, range (-180, 180]
        if (theta < 0) theta = 360 + theta; // range [0, 360)
        this.length = euclideanDistance(this.x1, this.x2, this.y1, this.y2)

        // SKIN por parede (SICRO): parede normal, muro, cerca ou calçada. O estilo
        // vem do mapa por par-de-nós; ausência = parede normal. Desenhado em coords
        // locais (x 0..length, eixo central cy), em escala — a cota segue valendo.
        const __style =
            wallStyleMap.get(
                wallStyleKey(this.leftNode.getId(), this.rightNode.getId()),
            ) || "parede";
        const L = this.length;
        const cy = this.thickness / 2;
        if (__style === "calcada") {
            const w = 120;
            this.lineStyle(1.2, 0x9aa3ad, 1);
            this.beginFill(0xe6e6e6, 1).drawRect(0, cy - w / 2, L, w).endFill();
        } else if (__style === "muro") {
            const w = Math.max(this.thickness, 16);
            this.lineStyle(1.5, 0x3f3f3f, 1);
            this.beginFill(0xbdbdbd, 1).drawRect(0, cy - w / 2, L, w).endFill();
            this.lineStyle(1, 0x6b6b6b, 0.6); // hachura leve
            for (let d = 0; d <= L; d += 14) {
                this.moveTo(d, cy - w / 2);
                this.lineTo(Math.min(d + 6, L), cy + w / 2);
            }
        } else if (__style === "cerca_madeira") {
            this.lineStyle(3.5, 0x8a5a2b, 1);
            this.moveTo(0, cy);
            this.lineTo(L, cy);
            this.lineStyle(3, 0x6e4420, 1); // moirões
            for (let d = 0; d <= L; d += 46) {
                this.moveTo(d, cy - 9);
                this.lineTo(d, cy + 9);
            }
        } else if (__style === "cerca_arame") {
            this.lineStyle(2, 0x707070, 1); // tracejado
            const dash = 16;
            const gap = 10;
            let d = 0;
            while (d < L) {
                const e = Math.min(d + dash, L);
                this.moveTo(d, cy);
                this.lineTo(e, cy);
                d = e + gap;
            }
        } else {
            this.lineStyle(1, 0x1a1a1a, 1);
            this.beginFill().drawRect(0, 0, L, this.thickness).endFill();
        }
        // Área de clique RETANGULAR fixa (independe da skin). Sem isto, as
        // cercas — que só desenham linhas, sem preenchimento — não têm área
        // hittável e a ferramenta Remover (que depende do clique na parede) não
        // pegava. Cobre a faixa da parede com uma folga mínima de 28px.
        const hitH = Math.max(this.thickness, 28);
        this.hitArea = new Rectangle(0, cy - hitH / 2, L, hitH);

        this.position.set(this.x1, this.y1)
        this.angle = theta

        this.leftNode.angle = theta;
        this.rightNode.angle = theta;

        // Offset da cota (arrastada pelo perito), em coords locais da parede.
        const __lkey = wallStyleKey(this.leftNode.getId(), this.rightNode.getId());
        const __loff = labelOffsetMap.get(__lkey) || { x: 0, y: 0 };
        this.label.update(this.length - WALL_THICKNESS);
        this.label.position.x = this.width / 2 + __loff.x;
        this.label.angle = 360 - theta

        this.label.position.y = 25 + __loff.y;
        this.label.zIndex = 998;

    }

    private onRightDown(ev: InteractionEvent) {
        ev.stopPropagation();
        this.setIsExterior(!this.isExteriorWall);
        return
    }


    private onMouseMove(ev: InteractionEvent) {
        if (!this.dragging) {
            return;
        }
        let currentPoint = ev.data.global;
        let delta = {
            x: currentPoint.x - this.mouseStartPoint.x,
            y: currentPoint.y - this.mouseStartPoint.y
        }

        this.leftNode.setPosition(this.startLeftNode.x + delta.x, this.startLeftNode.y + delta.y);
        this.rightNode.setPosition(this.startRightNode.x + delta.x, this.startRightNode.y + delta.y);
    }

    private onMouseUp(ev: InteractionEvent) {
        this.dragging = false;
        return;
    }

    private onMouseDown(ev: InteractionEvent) {
        ev.stopPropagation();

        let coords = {x:viewportX(ev.data.global.x), y:viewportY(ev.data.global.y)}
        let localCoords = ev.data.getLocalPosition(this)

        const state = useStore.getState()

        if (state.activeTool == Tool.Remove) {

            let action = new DeleteWallAction(this);
            action.execute();
        }

        if (state.activeTool == Tool.WallAdd) {
            const addNode = new AddNodeAction(this, coords);
            addNode.execute();
        }
        if (state.activeTool == Tool.FurnitureAddWindow) {
            getWindow().then(res => {
                let action = new AddFurnitureAction(res[0], this, {x:localCoords.x, y:0}, this.leftNode.getId(), this.rightNode.getId());
                action.execute();
            })

        }

        if (state.activeTool == Tool.FurnitureAddDoor) {
            getDoor().then(res => {
                let action = new AddFurnitureAction(res[0], this, {x:localCoords.x, y:0}, this.leftNode.getId(), this.rightNode.getId());
                action.execute();
            })
        }

        if (state.activeTool == Tool.Edit && !this.dragging) {
            this.dragging = true;
            this.mouseStartPoint.x = viewportX(ev.data.global.x);
            this.mouseStartPoint.y = viewportY(ev.data.global.y);
            this.startLeftNode.x = this.leftNode.position.x;
            this.startLeftNode.y = this.leftNode.position.y;

            this.startRightNode.x = this.rightNode.position.x;
            this.startRightNode.y = this.rightNode.position.y;

            return;
        }

    }

    /**
     * Define o comprimento da parede (em px) digitando a medida — mantém o
     * centro e o ângulo, movendo os dois nós simetricamente. (port arcada PR #14,
     * SSakibHossain10) Usado pela edição da cota (Label).
     */
    public updateWallLength(size: number) {
        if (!isFinite(size) || size <= 0) return; // campo vazio/zero → ignora

        const currentLength = Math.sqrt(Math.pow(this.x2 - this.x1, 2) + Math.pow(this.y2 - this.y1, 2));
        const angleInRadians = Math.atan2(this.y2 - this.y1, this.x2 - this.x1);

        const deltaX = ((size - currentLength) / 2) * Math.cos(angleInRadians);
        const deltaY = ((size - currentLength) / 2) * Math.sin(angleInRadians);

        const newX1 = this.x1 - deltaX;
        const newY1 = this.y1 - deltaY;
        const newX2 = this.x2 + deltaX;
        const newY2 = this.y2 + deltaY;

        if (this.leftNode.x <= this.rightNode.x) {
            this.leftNode.x = newX1; this.leftNode.y = newY1;
            this.rightNode.x = newX2; this.rightNode.y = newY2;
        } else {
            this.leftNode.x = newX2; this.leftNode.y = newY2;
            this.rightNode.x = newX1; this.rightNode.y = newY1;
        }

        FloorPlan.Instance.redrawWalls();
    }

}