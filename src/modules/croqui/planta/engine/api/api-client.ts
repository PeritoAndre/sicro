/**
 * Catálogo LOCAL e OFFLINE de mobília / portas / janelas do croqui de planta.
 *
 * Substitui o `api-client` do arcada, que buscava categorias e definições de
 * porta/janela de um servidor (localhost:4133 / MongoDB). §13: tudo offline,
 * com assets ORIGINAIS (SVG top-view) bundlados pelo Vite. A assinatura das
 * funções é mantida compatível com os consumidores do motor (que faziam
 * `fetch(...).json()`).
 */

import camaUrl from "../../assets/2d/cama.svg?url";
import sofaUrl from "../../assets/2d/sofa.svg?url";
import mesaUrl from "../../assets/2d/mesa.svg?url";
import cadeiraUrl from "../../assets/2d/cadeira.svg?url";
import fogaoUrl from "../../assets/2d/fogao.svg?url";
import geladeiraUrl from "../../assets/2d/geladeira.svg?url";
import vasoUrl from "../../assets/2d/vaso.svg?url";
import carroUrl from "../../assets/2d/carro.svg?url";
import portaUrl from "../../assets/2d/porta.svg?url";
import janelaUrl from "../../assets/2d/janela.svg?url";
import pessoaPeUrl from "../../assets/2d/pessoa_pe.svg?url";
import pessoaCaidoUrl from "../../assets/2d/pessoa_caido.svg?url";
import pessoaSentadoUrl from "../../assets/2d/pessoa_sentado.svg?url";

export interface FurnitureDef {
  _id: string;
  name: string;
  width: number; // metros
  height: number; // metros
  imagePath: string; // URL resolvida pelo Vite
  category?: string;
  zIndex?: number;
}

export interface CategoryDef {
  _id: string;
  name: string;
  visible: boolean;
}

const CATEGORIES: CategoryDef[] = [
  { _id: "comodo", name: "Quarto", visible: true },
  { _id: "sala", name: "Sala / Estar", visible: true },
  { _id: "cozinha", name: "Cozinha", visible: true },
  { _id: "banheiro", name: "Banheiro", visible: true },
  { _id: "externo", name: "Externo", visible: true },
];

const FURNITURE: Record<string, FurnitureDef[]> = {
  comodo: [
    { _id: "cama", name: "Cama (casal)", width: 1.4, height: 1.9, imagePath: camaUrl, category: "comodo" },
    { _id: "guarda_roupa", name: "Guarda-roupa", width: 1.8, height: 0.6, imagePath: geladeiraUrl, category: "comodo" },
  ],
  sala: [
    { _id: "sofa", name: "Sofá", width: 1.8, height: 0.9, imagePath: sofaUrl, category: "sala" },
    { _id: "mesa", name: "Mesa", width: 1.2, height: 0.8, imagePath: mesaUrl, category: "sala" },
    { _id: "cadeira", name: "Cadeira", width: 0.5, height: 0.5, imagePath: cadeiraUrl, category: "sala" },
  ],
  cozinha: [
    { _id: "fogao", name: "Fogão", width: 0.6, height: 0.6, imagePath: fogaoUrl, category: "cozinha" },
    { _id: "geladeira", name: "Geladeira", width: 0.7, height: 0.7, imagePath: geladeiraUrl, category: "cozinha" },
    { _id: "mesa_cozinha", name: "Mesa", width: 1.0, height: 0.8, imagePath: mesaUrl, category: "cozinha" },
  ],
  banheiro: [
    { _id: "vaso", name: "Vaso sanitário", width: 0.4, height: 0.6, imagePath: vasoUrl, category: "banheiro" },
  ],
  externo: [
    { _id: "carro", name: "Veículo", width: 1.8, height: 4.5, imagePath: carroUrl, category: "externo" },
  ],
};

const DOOR: FurnitureDef[] = [
  { _id: "porta", name: "Porta", width: 0.9, height: 0.9, imagePath: portaUrl, zIndex: 5 },
];

const WINDOW: FurnitureDef[] = [
  { _id: "janela", name: "Janela", width: 1.2, height: 0.2, imagePath: janelaUrl, zIndex: 5 },
];

// Pessoas (vista de cima) — colocadas como mobília (move/gira/persiste/exporta).
// Placeholders; o perito substitui a arte depois. zIndex alto p/ ficar acima.
const PEOPLE: FurnitureDef[] = [
  { _id: "pessoa_pe", name: "Pessoa — em pé", width: 0.5, height: 0.5, imagePath: pessoaPeUrl, category: "pessoa", zIndex: 8 },
  { _id: "pessoa_caido", name: "Pessoa — caída", width: 0.5, height: 1.7, imagePath: pessoaCaidoUrl, category: "pessoa", zIndex: 8 },
  { _id: "pessoa_sentado", name: "Pessoa — sentada", width: 0.5, height: 0.6, imagePath: pessoaSentadoUrl, category: "pessoa", zIndex: 8 },
];

/** Poses de pessoa (vista de cima) — usadas pelo botão "Pessoas" da planta. */
export function getPeople(): FurnitureDef[] {
  return PEOPLE;
}

/** Lista de categorias (compat fetch().json()). */
export function getCategoriesRequest(): Promise<{ json: () => Promise<CategoryDef[]> }> {
  return Promise.resolve({ json: async () => CATEGORIES });
}

/** Mobília de uma categoria (compat fetch().json()). */
export function getCategoryInfo(
  categoryId: string,
): Promise<{ json: () => Promise<FurnitureDef[]> }> {
  const list = FURNITURE[categoryId] ?? [];
  return Promise.resolve({ json: async () => list });
}

/** Definição da porta (array — o motor usa res[0]). */
export async function getDoor(): Promise<FurnitureDef[]> {
  return DOOR;
}

/** Definição da janela (array — o motor usa res[0]). */
export async function getWindow(): Promise<FurnitureDef[]> {
  return WINDOW;
}
