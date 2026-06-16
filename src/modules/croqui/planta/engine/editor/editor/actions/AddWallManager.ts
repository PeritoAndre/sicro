// @ts-nocheck -- vendored do arcada (Apache-2.0), vide planta/ATTRIBUTION.md; interfaceado via tipos do SICRO
import { Graphics, InteractionEvent } from "pixi.js";
import { euclideanDistance } from "../../../helpers/EuclideanDistance";
import type { Point } from "../../../helpers/Point";
import { viewportX, viewportY } from "../../../helpers/ViewportCoordinates";
import { INTERIOR_WALL_THICKNESS, METER, WALL_THICKNESS } from "../constants";
import { FloorPlan } from "../objects/FloorPlan";
import { Label } from "../objects/TransformControls/Label";
import { TransformLayer } from "../objects/TransformControls/TransformLayer";
import { WallNode } from "../objects/Walls/WallNode";
import { AddWallAction } from "./AddWallAction";
import { Preview } from "./MeasureToolManager";

// tracks current action data
export class AddWallManager {


    private static instance: AddWallManager;

    public previousNode: WallNode;

    public preview: Preview;

    // Quantas paredes a sequência atual já criou. 0 = só o 1º clique (nó solto).
    private wallsInChain = 0;

    private constructor() {
        this.previousNode = undefined;
        this.preview = new Preview();


    }

    // checks if step is valid
    public checkStep(coords:Point) {
        if (this.previousNode == undefined) {
            for (let [id,node] of FloorPlan.Instance.getWallNodeSeq().getWallNodes()) {
                if (euclideanDistance(coords.x, node.x, coords.y, node.y) < 0.3 * METER) {
                    return false;
                }
            }
            return true;
        }

        if (euclideanDistance(coords.x, this.previousNode.x, coords.y, this.previousNode.y) < 0.3 * METER) {
            return false;
        }
        return true;
    }
    public step(node: WallNode) {
        // first click. set first node
        if (this.previousNode === undefined) {
            this.previousNode = node;
            this.wallsInChain = 0;
            this.preview.set(this.previousNode.position)
            return;
        }

        // double click. end chain
        if (this.previousNode.getId() === node.getId()) {
            this.previousNode = undefined;
            this.wallsInChain = 0;
            this.preview.set(undefined)
            return;
        }

        //new node on screen
        let wallAction = new AddWallAction(this.previousNode, node);
        wallAction.execute();
        this.wallsInChain++;
        this.preview.set(node.position)

        this.previousNode = node;
        this.preview.set(this.previousNode.position)
        // this.sizeLabel.visible = false;

    }

    /**
     * Cancela a sequência em andamento. Se só houve o 1º clique (nó solto, sem
     * parede), remove o nó órfão do floorplan — senão deixaria um ponto perdido.
     * Usado pelo botão direito e ao trocar de ferramenta.
     */
    public cancelChain() {
        if (this.previousNode !== undefined && this.wallsInChain === 0) {
            try {
                FloorPlan.Instance.removeWallNode(this.previousNode.getId());
                FloorPlan.Instance.redrawWalls();
            } catch {
                /* noop */
            }
        }
        this.previousNode = undefined;
        this.wallsInChain = 0;
        this.preview.set(undefined);
    }

    public updatePreview(ev:InteractionEvent) {
        this.preview.updatePreview(ev, true);

    }
    public unset() {
        this.previousNode = undefined;
        this.wallsInChain = 0;
        this.preview.set(undefined);
    }

    /**
     * true quando a sequência tem só o 1º clique (nó solto, sem parede ainda).
     * Esse estado é TRANSITÓRIO: não deve virar passo de histórico (senão o
     * Ctrl+Z volta pro "nó solto" e deixa um ponto órfão).
     */
    public isPendingLoneNode() {
        return this.previousNode !== undefined && this.wallsInChain === 0;
    }
    public static get Instance() {
        return this.instance || (this.instance = new this());
    }

    public resetTools() {
        TransformLayer.Instance.deselect();
        // Trocar de ferramenta cancela a sequência (e limpa o nó solto do 1º clique).
        this.cancelChain();
    }
}

