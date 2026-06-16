// @ts-nocheck -- vendored do arcada (Apache-2.0), vide planta/ATTRIBUTION.md; interfaceado via tipos do SICRO
import { FloorPlan } from "../objects/FloorPlan";
import { Wall } from "../objects/Walls/Wall";
import type { Action } from "./Action";

export class DeleteWallAction implements Action {

    private wall:Wall; //TODO: Add node data pt undo/redo
    private receiver:FloorPlan;

    constructor(wall:Wall) {
        this.wall = wall;
        this.receiver = FloorPlan.Instance;
    }

    public execute(): void {
        this.receiver.actions.push(this);
        this.receiver.removeWall(this.wall);
        
    }
}

