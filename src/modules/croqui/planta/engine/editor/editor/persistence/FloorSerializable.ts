// @ts-nocheck -- vendored do arcada (Apache-2.0), vide planta/ATTRIBUTION.md; interfaceado via tipos do SICRO
import type { IFurnitureSerializable } from "./IFurnitureSerializable";
import type { INodeSerializable } from "./INodeSerializable";

export class FloorSerializable {
    public furnitureArray: IFurnitureSerializable[];
    public wallNodes: INodeSerializable[];
    public wallNodeLinks: [number, number[]][];
    
    public constructor() {
        this.furnitureArray = [];
        this.wallNodes = [];
        this.wallNodeLinks = [];
    
    }

}