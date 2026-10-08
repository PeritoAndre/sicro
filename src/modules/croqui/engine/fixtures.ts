/**
 * Sinalização e entorno paramétricos (placas, semáforos, faixas, postes, árvores…),
 * no mesmo molde dos vestígios: px de mundo para posição, metros nos parâmetros.
 */

import type { FixtureSubtype, ObjectCategory, SicroFixtureObject, SicroPoint } from "./schema";
import { traceGeom, type TraceGeom, type TraceParamSpec, type TraceParamValue } from "./traces";

export type FixtureGrupo = "sinalizacao" | "entorno";

export interface FixtureSpec {
  nome: string;
  sigla: string;
  grupo: FixtureGrupo;
  /** "ponto": p0 = posição, p1 = direção/alcance. "linha": traçado p0 → p1. */
  forma: "ponto" | "linha";
  /** Ponto: o que a alça de p1 significa. "raio" edita `params.raio` (p1 fica em p0). */
  p1?: "direcao" | "alcance" | "raio";
  /** Linha: aceita flecha (curva). */
  curva?: boolean;
  /** Linha: comprimento padrão. Ponto: distância padrão de p1 (m). */
  len_m: number;
  params: TraceParamSpec[];
  def: Record<string, TraceParamValue>;
}

const TAM = (min = 0.6, max = 3): TraceParamSpec => ({ k: "tam", t: "range", label: "Tamanho do símbolo", min, max, step: 0.05, u: "m" });

export const FIXTURE_SPECS: Record<FixtureSubtype, FixtureSpec> = {
  placa: {
    nome: "Placa", sigla: "Pl", grupo: "sinalizacao", forma: "ponto", p1: "direcao", len_m: 2,
    params: [
      {
        k: "modelo", t: "seg", label: "Modelo",
        opts: [["pare", "PARE"], ["pref", "Preferência"], ["vel", "Velocidade"], ["proib_est", "Proib. estacionar"], ["adv", "Advertência"], ["ind", "Indicação"]],
      },
      { k: "vel", t: "range", label: "Velocidade máxima", min: 10, max: 120, step: 10, u: "km/h", when: ["modelo", ["vel"]] },
      { k: "texto", t: "txt", label: "Texto da placa", max: 24, when: ["modelo", ["adv", "ind"]] },
      TAM(),
    ],
    def: { modelo: "pare", vel: 40, texto: "", tam: 1.6 },
  },
  semaforo: {
    nome: "Semáforo", sigla: "Sm", grupo: "sinalizacao", forma: "ponto", p1: "alcance", len_m: 4,
    params: [
      { k: "grupo", t: "seg", label: "Grupo focal", opts: [["veic", "Veicular"], ["ped", "Pedestre"]] },
      { k: "fase", t: "seg", label: "Fase destacada", opts: [["nenhuma", "Nenhuma"], ["vermelho", "Vermelho"], ["amarelo", "Amarelo"], ["verde", "Verde"]] },
      { k: "braco", t: "chk", label: "Em braço projetado sobre a via" },
      TAM(0.6, 2.5),
    ],
    def: { grupo: "veic", fase: "nenhuma", braco: true, tam: 1.4 },
  },
  faixa_pedestre: {
    nome: "Faixa de pedestres", sigla: "FP", grupo: "sinalizacao", forma: "linha", len_m: 10,
    params: [
      { k: "largura", t: "range", label: "Largura da faixa", min: 3, max: 8, step: 0.1, u: "m" },
      { k: "listra", t: "range", label: "Largura das listras", min: 0.3, max: 0.5, step: 0.05, u: "m" },
      { k: "espaco", t: "range", label: "Espaço entre listras", min: 0.3, max: 0.8, step: 0.05, u: "m" },
    ],
    def: { largura: 4, listra: 0.4, espaco: 0.6 },
  },
  retencao: {
    nome: "Linha de retenção", sigla: "LR", grupo: "sinalizacao", forma: "linha", len_m: 7,
    params: [{ k: "esp", t: "range", label: "Espessura", min: 0.3, max: 0.6, step: 0.05, u: "m" }],
    def: { esp: 0.4 },
  },
  lombada: {
    nome: "Lombada", sigla: "Lb", grupo: "sinalizacao", forma: "linha", len_m: 7,
    params: [
      { k: "tipo", t: "seg", label: "Tipo (CONTRAN)", opts: [["A", "A · 3,7 m"], ["B", "B · 1,5 m"]] },
      { k: "pintura", t: "chk", label: "Pintura amarela (faixas a 45°)" },
    ],
    def: { tipo: "A", pintura: false },
  },
  area_conflito: {
    nome: "Área de conflito", sigla: "AC", grupo: "sinalizacao", forma: "linha", len_m: 10,
    params: [
      { k: "largura", t: "range", label: "Largura da área", min: 1, max: 40, step: 0.5, u: "m" },
      { k: "passo", t: "range", label: "Espaço entre as diagonais", min: 1, max: 4, step: 0.1, u: "m" },
      { k: "rot", t: "range", label: "Rotação", min: 0, max: 359, step: 1, u: "°" },
    ],
    def: { largura: 10, passo: 2 },
  },
  seta: {
    nome: "Seta no pavimento", sigla: "St", grupo: "sinalizacao", forma: "ponto", p1: "direcao", len_m: 3,
    params: [
      {
        k: "tipo", t: "seg", label: "Movimento",
        opts: [["frente", "Frente"], ["esq", "Esquerda"], ["dir", "Direita"], ["frente_esq", "Frente/esq."], ["frente_dir", "Frente/dir."], ["retorno", "Retorno"]],
      },
      { k: "comp", t: "range", label: "Comprimento", min: 3, max: 7.5, step: 0.1, u: "m" },
    ],
    def: { tipo: "frente", comp: 5 },
  },
  poste: {
    nome: "Poste", sigla: "Pt", grupo: "entorno", forma: "ponto", p1: "alcance", len_m: 2.5,
    params: [
      { k: "diam", t: "range", label: "Diâmetro", min: 0.15, max: 0.6, step: 0.01, u: "m" },
      { k: "luminaria", t: "chk", label: "Luminária (braço até a alça)" },
    ],
    def: { diam: 0.3, luminaria: false },
  },
  arvore: {
    nome: "Árvore", sigla: "Av", grupo: "entorno", forma: "ponto", p1: "raio", len_m: 0,
    params: [
      { k: "raio", t: "range", label: "Raio da copa", min: 0.8, max: 8, step: 0.1, u: "m" },
      { k: "tronco", t: "range", label: "Diâmetro do tronco", min: 0.15, max: 1.2, step: 0.05, u: "m" },
      { k: "ramos", t: "chk", label: "Ramos na copa" },
    ],
    def: { raio: 2.5, tronco: 0.4, ramos: true },
  },
  hidrante: {
    nome: "Hidrante", sigla: "Hd", grupo: "entorno", forma: "ponto", p1: "direcao", len_m: 1,
    params: [TAM(0.3, 1.5)],
    def: { tam: 0.6 },
  },
  abrigo: {
    nome: "Ponto de ônibus", sigla: "PO", grupo: "entorno", forma: "ponto", p1: "direcao", len_m: 2.5,
    params: [
      { k: "comp", t: "range", label: "Comprimento", min: 2, max: 10, step: 0.1, u: "m" },
      { k: "prof", t: "range", label: "Profundidade", min: 1, max: 2.5, step: 0.05, u: "m" },
    ],
    def: { comp: 4, prof: 1.6 },
  },
  barreira: {
    nome: "Barreira", sigla: "Br", grupo: "entorno", forma: "linha", curva: true, len_m: 12,
    params: [
      { k: "tipo", t: "seg", label: "Tipo", opts: [["defensa", "Defensa metálica"], ["new_jersey", "New Jersey"], ["muro", "Muro"], ["gradil", "Gradil / cerca"]] },
      { k: "esp", t: "range", label: "Espessura do muro", min: 0.1, max: 0.8, step: 0.05, u: "m", when: ["tipo", ["muro"]] },
      { k: "bend", t: "range", label: "Curvatura (flecha)", min: -6, max: 6, step: 0.05, u: "m" },
    ],
    def: { tipo: "defensa", esp: 0.25 },
  },
  obstaculo: {
    nome: "Obstáculo", sigla: "Ob", grupo: "entorno", forma: "ponto", p1: "direcao", len_m: 2,
    params: [
      { k: "forma", t: "seg", label: "Forma", opts: [["ret", "Retângulo"], ["circ", "Círculo"]] },
      { k: "larg", t: "range", label: "Comprimento / diâmetro", min: 0.3, max: 15, step: 0.1, u: "m" },
      { k: "prof", t: "range", label: "Largura", min: 0.3, max: 15, step: 0.1, u: "m", when: ["forma", ["ret"]] },
      { k: "hachura", t: "chk", label: "Hachura" },
    ],
    def: { forma: "ret", larg: 2.5, prof: 1.5, hachura: true },
  },
  camera: {
    nome: "Câmera", sigla: "Cm", grupo: "entorno", forma: "ponto", p1: "alcance", len_m: 15,
    params: [
      { k: "abert", t: "range", label: "Abertura do campo", min: 15, max: 140, step: 1, u: "°" },
      { k: "campo", t: "chk", label: "Mostrar campo de visão" },
    ],
    def: { abert: 60, campo: true },
  },
};

export const FIXTURE_SUBTYPES = Object.keys(FIXTURE_SPECS) as FixtureSubtype[];

export function isFixtureSubtype(v: string): v is FixtureSubtype {
  return v in FIXTURE_SPECS;
}

export function fixtureCategory(subtype: FixtureSubtype): ObjectCategory {
  return FIXTURE_SPECS[subtype]?.grupo === "entorno" ? "entorno" : "sinalizacao";
}

export function fp<T extends TraceParamValue>(o: SicroFixtureObject, k: string): T {
  const v = o.params?.[k];
  return (v === undefined ? FIXTURE_SPECS[o.subtype]?.def[k] : v) as T;
}
export const fn = (o: SicroFixtureObject, k: string): number => Number(fp(o, k)) || 0;

/** `p0` = posição (ou início da linha); sem `p1`, a direção padrão é +x. */
export function makeFixture(subtype: FixtureSubtype, p0: SicroPoint, p1: SicroPoint | null, pxPerM: number): SicroFixtureObject {
  const spec = FIXTURE_SPECS[subtype];
  const end =
    spec.p1 === "raio" ? { ...p0 } : p1 && Math.hypot(p1.x - p0.x, p1.y - p0.y) > 1 ? { ...p1 } : { x: p0.x + spec.len_m * pxPerM, y: p0.y };
  return {
    id: `fixture_${typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`}`,
    layer_id: "layer_objects",
    kind: "fixture",
    subtype,
    p0: { ...p0 },
    p1: end,
    bend: 0,
    seed: Math.floor(Math.random() * 1e6) + 1,
    params: { ...spec.def },
    label: "",
    visible: true,
    locked: false,
    category: fixtureCategory(subtype),
  };
}

/** Direção unitária p0 → p1 (padrão +x) e distância (m). */
export function fixtureDir(o: SicroFixtureObject, pxPerM: number): { ux: number; uy: number; dist: number } {
  const dx = o.p1.x - o.p0.x;
  const dy = o.p1.y - o.p0.y;
  const d = Math.hypot(dx, dy);
  return d < 1e-6 ? { ux: 1, uy: 0, dist: 0 } : { ux: dx / d, uy: dy / d, dist: d / pxPerM };
}

export function fixtureGeomM(o: SicroFixtureObject, pxPerM: number): TraceGeom {
  const k = 1 / Math.max(pxPerM, 1e-6);
  return traceGeom({ x: o.p0.x * k, y: o.p0.y * k }, { x: o.p1.x * k, y: o.p1.y * k }, (o.bend ?? 0) * k);
}

/** Meia largura (linha) ou raio do símbolo (ponto), em metros. */
export function fixtureExtentM(o: SicroFixtureObject): number {
  switch (o.subtype) {
    case "placa":
    case "semaforo":
    case "hidrante":
      return fn(o, "tam") * 0.75;
    case "faixa_pedestre":
      return fn(o, "largura") / 2;
    case "retencao":
      return fn(o, "esp") / 2;
    case "lombada":
      return (fp(o, "tipo") === "B" ? 1.5 : 3.7) / 2;
    case "area_conflito":
      return fn(o, "largura") / 2;
    case "seta":
      return fn(o, "comp") / 2;
    case "poste":
      return Math.max(0.4, fn(o, "diam"));
    case "arvore":
      return fn(o, "raio");
    case "abrigo":
      return Math.hypot(fn(o, "comp"), fn(o, "prof")) / 2;
    case "barreira":
      return fp(o, "tipo") === "new_jersey" ? 0.35 : fp(o, "tipo") === "muro" ? fn(o, "esp") / 2 : 0.2;
    case "obstaculo":
      return fp(o, "forma") === "circ" ? fn(o, "larg") / 2 : Math.hypot(fn(o, "larg"), fn(o, "prof")) / 2;
    case "camera":
      return 0.5;
    default:
      return 0.5;
  }
}

const fmt = (v: number, d = 1) => v.toFixed(d).replace(".", ",");

export function fixtureReadouts(o: SicroFixtureObject, pxPerM: number): { rows: [string, string][] } {
  const spec = FIXTURE_SPECS[o.subtype];
  if (spec.forma === "linha") {
    const g = fixtureGeomM(o, pxPerM);
    return { rows: [[o.subtype === "faixa_pedestre" ? "Travessia" : "Comprimento", `${fmt(g.len)} m`]] };
  }
  const { dist } = fixtureDir(o, pxPerM);
  if (o.subtype === "camera") return { rows: [["Alcance do campo", `${fmt(dist)} m`], ["Abertura", `${Math.round(fn(o, "abert"))}°`]] };
  if (o.subtype === "semaforo" && fp(o, "braco")) return { rows: [["Braço", `${fmt(dist)} m`]] };
  if (o.subtype === "poste" && fp(o, "luminaria")) return { rows: [["Braço da luminária", `${fmt(dist)} m`]] };
  if (o.subtype === "arvore") return { rows: [["Diâmetro da copa", `${fmt(fn(o, "raio") * 2)} m`]] };
  return { rows: [] };
}

/** AABB em px de mundo. */
export function fixtureBoundsPx(o: SicroFixtureObject, pxPerM: number): { x: number; y: number; width: number; height: number } {
  const pad = fixtureExtentM(o) * pxPerM;
  const spec = FIXTURE_SPECS[o.subtype];
  let x0 = Math.min(o.p0.x, spec.forma === "linha" || spec.p1 === "alcance" ? o.p1.x : o.p0.x);
  let x1 = Math.max(o.p0.x, spec.forma === "linha" || spec.p1 === "alcance" ? o.p1.x : o.p0.x);
  let y0 = Math.min(o.p0.y, spec.forma === "linha" || spec.p1 === "alcance" ? o.p1.y : o.p0.y);
  let y1 = Math.max(o.p0.y, spec.forma === "linha" || spec.p1 === "alcance" ? o.p1.y : o.p0.y);
  if (spec.forma === "linha" && o.bend) {
    const g = fixtureGeomM(o, pxPerM);
    for (const p of g.pts) {
      x0 = Math.min(x0, p.x * pxPerM);
      x1 = Math.max(x1, p.x * pxPerM);
      y0 = Math.min(y0, p.y * pxPerM);
      y1 = Math.max(y1, p.y * pxPerM);
    }
  }
  return { x: x0 - pad, y: y0 - pad, width: x1 - x0 + 2 * pad, height: y1 - y0 + 2 * pad };
}
