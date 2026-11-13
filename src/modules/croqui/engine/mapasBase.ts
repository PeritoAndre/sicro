/**
 * Mapas base para o fundo do croqui (a mesma lista do HUB, por grupo). Esri, OSM, NASA e EOX bastam com atribuição;
 * Google e Bing saem sem chave e fora dos termos do fornecedor (decisão do dono, 13/11/2026: igual ao HUB).
 */

export type GrupoDoMapaBase = "imagem" | "ruas" | "relevo" | "neutros";

export interface MapaBase {
  id: string;
  rotulo: string;
  grupo: GrupoDoMapaBase;
  /** Modelos com {z} {x} {y} ({q} = quadkey do Bing); vários = subdomínios em rodízio. */
  tiles: string[];
  atrib: string;
  /** Último nível que o servidor tem. */
  maxzoom: number;
}

const ESRI = "https://server.arcgisonline.com/ArcGIS/rest/services";
const esri = (nome: string) => `${ESRI}/${nome}/MapServer/tile/{z}/{y}/{x}`;
const sub = (modelo: string, letras: string) => [...letras].map((s) => modelo.replace("{s}", s));
const GOOGLE = (lyrs: string) => sub(`https://mt{s}.google.com/vt/lyrs=${lyrs}&x={x}&y={y}&z={z}`, "0123");
const ATRIB_ESRI = "Esri, Maxar, Earthstar Geographics";
const ATRIB_GOOGLE = "© Google";

/** A Esri só tem imagem até o zoom 17 no Amapá; os cinzas, até o 15. */
export const MAPAS_BASE: MapaBase[] = [
  { id: "google-satelite", rotulo: "Google Satélite", grupo: "imagem", tiles: GOOGLE("s"), atrib: ATRIB_GOOGLE, maxzoom: 20 },
  { id: "google-hibrido", rotulo: "Google Satélite com nomes", grupo: "imagem", tiles: GOOGLE("y"), atrib: ATRIB_GOOGLE, maxzoom: 20 },
  { id: "bing", rotulo: "Bing Satélite", grupo: "imagem", tiles: sub("https://ecn.t{s}.tiles.virtualearth.net/tiles/a{q}.jpeg?g=1", "0123"), atrib: "© Microsoft Bing", maxzoom: 19 },
  { id: "satelite", rotulo: "Esri Satélite", grupo: "imagem", tiles: [esri("World_Imagery")], atrib: ATRIB_ESRI, maxzoom: 17 },
  { id: "sentinel", rotulo: "Sentinel-2 (2024)", grupo: "imagem", tiles: ["https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless-2024_3857/default/g/{z}/{y}/{x}.jpg"], atrib: "Sentinel-2 cloudless © EOX (Copernicus), CC BY-NC", maxzoom: 17 },
  { id: "nasa-noite", rotulo: "NASA noturno", grupo: "imagem", tiles: ["https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/VIIRS_Black_Marble/default/2016-01-01/GoogleMapsCompatible_Level8/{z}/{y}/{x}.png"], atrib: "NASA GIBS, Black Marble (VIIRS)", maxzoom: 8 },
  { id: "google-mapas", rotulo: "Google Maps", grupo: "ruas", tiles: GOOGLE("m"), atrib: ATRIB_GOOGLE, maxzoom: 20 },
  { id: "osm", rotulo: "OpenStreetMap", grupo: "ruas", tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"], atrib: "© OpenStreetMap", maxzoom: 19 },
  { id: "esri-ruas", rotulo: "Esri Street", grupo: "ruas", tiles: [esri("World_Street_Map")], atrib: "Esri, HERE, Garmin, OpenStreetMap", maxzoom: 19 },
  { id: "esri-topo", rotulo: "Esri Topographic", grupo: "relevo", tiles: [esri("World_Topo_Map")], atrib: "Esri, HERE, Garmin, USGS, NGA", maxzoom: 19 },
  { id: "google-terreno", rotulo: "Google Terrain", grupo: "relevo", tiles: GOOGLE("p"), atrib: ATRIB_GOOGLE, maxzoom: 19 },
  { id: "opentopomap", rotulo: "OpenTopoMap", grupo: "relevo", tiles: sub("https://{s}.tile.opentopomap.org/{z}/{x}/{y}.png", "abc"), atrib: "© OpenTopoMap (CC-BY-SA), © OpenStreetMap", maxzoom: 17 },
  { id: "claro", rotulo: "Esri Claro", grupo: "neutros", tiles: [esri("Canvas/World_Light_Gray_Base")], atrib: "Esri", maxzoom: 15 },
  { id: "esri-escuro", rotulo: "Esri Escuro", grupo: "neutros", tiles: [esri("Canvas/World_Dark_Gray_Base")], atrib: "Esri", maxzoom: 15 },
];

export const GRUPOS_DE_MAPA_BASE: ReadonlyArray<{ id: GrupoDoMapaBase; rotulo: string }> = [
  { id: "imagem", rotulo: "Imagem" },
  { id: "ruas", rotulo: "Ruas" },
  { id: "relevo", rotulo: "Relevo" },
  { id: "neutros", rotulo: "Neutros" },
];

export const mapaBasePorId = (id: string): MapaBase | undefined => MAPAS_BASE.find((m) => m.id === id);

/** Quadkey do Bing: o endereço do tile é a trilha na quadtree. */
export function quadkey(z: number, x: number, y: number): string {
  let out = "";
  for (let i = z; i > 0; i--) {
    const mask = 1 << (i - 1);
    out += String((x & mask ? 1 : 0) + (y & mask ? 2 : 0));
  }
  return out;
}

/** Endereço do tile; subdomínio pelo (x + y) para espalhar. */
export function urlDoTile(mapa: MapaBase, z: number, x: number, y: number): string {
  const modelo = mapa.tiles[(x + y) % mapa.tiles.length]!;
  return modelo
    .replace("{z}", String(z))
    .replace("{x}", String(x))
    .replace("{y}", String(y))
    .replace("{q}", quadkey(z, x, y));
}
