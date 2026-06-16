// @ts-nocheck -- vendored do arcada (Apache-2.0), vide planta/ATTRIBUTION.md; interfaceado via tipos do SICRO
import { FloorSerializable } from "./FloorSerializable"

export class FloorPlanSerializable {
    floors: FloorSerializable[];
    public furnitureId: number;
    public wallNodeId: number;
    
    constructor() {
        this.floors = [];
    }
}