/** Public surface of the Croqui Engine — framework-agnostic.
 *
 * Fase S clean cut — Road v1 e Road v2 foram completamente removidos.
 * O único motor de via/rotatória é o Python Parity Engine
 * (`./road-parity/*`).
 *
 * O parity expõe seus próprios tipos, factories, guards, geometria,
 * clipping, renderer e adapter OSM. Quem precisar deles importa
 * diretamente de `road-parity/...`.
 */

export * from "./schema";
export * from "./geometry";
export * from "./serializer";
export * from "./factories";
export * from "./templates";
export * from "./coordinates";
export * from "./osm";
