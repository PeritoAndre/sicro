// @ts-nocheck -- vendored do arcada (Apache-2.0), vide planta/ATTRIBUTION.md; interfaceado via tipos do SICRO
import { Floor } from "../objects/Floor";
import { FloorPlanSerializable } from "./FloorPlanSerializable";

export class Serializer {


    public serialize(floors: Floor[], furnitureId:number) {
        let floorPlanSerializable = new FloorPlanSerializable();
        for (let floor of floors) {
            let floorSerializable = floor.serialize();
            floorPlanSerializable.floors.push(floorSerializable)

        }
        floorPlanSerializable.furnitureId = furnitureId;
        floorPlanSerializable.wallNodeId = floors[0].getWallNodeSequence().getWallNodeId();
        let resultString = JSON.stringify(floorPlanSerializable)
        return resultString
    }
}