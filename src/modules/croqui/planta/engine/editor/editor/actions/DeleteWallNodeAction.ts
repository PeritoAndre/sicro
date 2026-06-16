// @ts-nocheck -- vendored do arcada (Apache-2.0), vide planta/ATTRIBUTION.md; interfaceado via tipos do SICRO
import { FloorPlan } from "../objects/FloorPlan";
import type { Action } from "./Action";

export class DeleteWallNodeAction implements Action {

    private id:number;
    private receiver:FloorPlan;
    constructor(id:number) {
        this.id = id;
        this.receiver = FloorPlan.Instance;
    }

    public execute(): void {
        this.receiver.actions.push(this);
        this.receiver.removeWallNode(this.id);
        
    }
}

