// @ts-nocheck -- vendored do arcada (Apache-2.0), vide planta/ATTRIBUTION.md; interfaceado via tipos do SICRO
import type { Point } from "../../../helpers/Point";
import type { FurnitureData } from "../../../stores/FurnitureStore";
import { FloorPlan } from "../objects/FloorPlan";
import { Wall } from "../objects/Walls/Wall";
import type { Action } from "./Action";

export class AddFurnitureAction implements Action{

    obj: FurnitureData;
    attachedTo: Wall;
    coords: Point;
    attachedToLeft: number;
    attachedToRight: number;
    private receiver:FloorPlan;

    constructor(obj: FurnitureData, attachedTo?: Wall, coords?: Point, attachedToLeft?:number, attachedToRight?:number) {
        this.obj = obj;
        this.attachedTo = attachedTo;
        this.coords = coords;
        this.attachedToLeft = attachedToLeft;
        this.attachedToRight = attachedToRight;
        this.receiver = FloorPlan.Instance;

    }


    public execute() {
        this.receiver.addFurniture(this.obj, this.attachedTo, this.coords, this.attachedToLeft, this.attachedToRight);
        this.receiver.actions.push(this);
    }
}