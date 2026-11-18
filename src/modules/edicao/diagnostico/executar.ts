/**
 * Diagnóstico de vídeo (P0.9, arquitetura §7.7): um clique prepara as mídias, roda `tempo3`, `pool8`, `compose`,
 * `switch` e `linha` (P4.1) no webview do app e grava `diagnostico-<data>.json`. Falha numa etapa fica no JSON e não para as outras.
 */
import { toSicroError } from "@core/errors";
import { type Ambiente, Cancelado, esperar, textoDeErro } from "./base";
import { compose } from "./compose";
import { pool8 } from "./pool8";
import { trocaDeClipe } from "./switch";
import { COPIAS_TEMPO3, FONTES_TEMPO3, tempo3 } from "./tempo3";
import type { MidiasDiagnostico } from "./comandos";

export const ESQUEMA_DIAGNOSTICO = "sicro.edicao.diagnostico/1";
export const MEDIDAS = ["tempo3", "pool8", "compose", "switch", "linha"] as const;
export type Medida = (typeof MEDIDAS)[number];

export interface Etapa {
  medida: Medida;
  chave: string;
  rodar(amb: Ambiente, midias: MidiasDiagnostico): Promise<unknown>;
}

const p0 = (m: MidiasDiagnostico, nome: string) => m.midias.find((x) => x.nome === nome)?.p0_s;

/** As variantes das rodadas do dossiê (`lista-tempo3*.json`, `lista-pool8.json`, `t=compose`, `lista-7.json`). */
export const PLANO: Etapa[] = [
  { medida: "tempo3", chave: "fontes", rodar: (amb) => tempo3(amb, FONTES_TEMPO3) },
  { medida: "tempo3", chave: "copias", rodar: (amb, m) => tempo3(amb, COPIAS_TEMPO3, COPIAS_TEMPO3.map((n) => p0(m, n))) },
  { medida: "pool8", chave: "p720", rodar: (amb) => pool8(amb, { suf: "_p720" }) },
  { medida: "pool8", chave: "1080p", rodar: (amb) => pool8(amb, { suf: "" }) },
  { medida: "pool8", chave: "p720_n2", rodar: (amb) => pool8(amb, { suf: "_p720", n: 2 }) },
  { medida: "compose", chave: "v1080_g30", rodar: (amb) => compose(amb, { f: "v1080_g30.mp4" }) },
  { medida: "compose", chave: "cam1_p720", rodar: (amb) => compose(amb, { f: "cam1_p720.mp4" }) },
  { medida: "switch", chave: "padrao", rodar: (amb) => trocaDeClipe(amb) },
  { medida: "switch", chave: "mesmo_arquivo", rodar: (amb) => trocaDeClipe(amb, { fa: "cam1.mp4", fb: "cam1.mp4" }) },
  { medida: "switch", chave: "velb_0_5", rodar: (amb) => trocaDeClipe(amb, { velb: 0.5 }) },
  // linha do tempo com 200 clipes (P4.1, `linha/diagnostico.tsx::ETAPA_LINHA`); carregada só ao medir
  { medida: "linha", chave: "200", rodar: async (amb) => (await import("../linha/diagnostico")).ETAPA_LINHA.rodar(amb) },
];

export type Estado = "espera" | "rodando" | "feita" | "falhou";
export type Linha = "midias" | Medida;
export interface Progresso {
  estados: Record<Linha, Estado>;
  /** Etapas de medição feitas / total. */
  feitas: number;
  total: number;
  /** Mídias geradas / total (evento do backend); null antes do primeiro. */
  midias: [number, number] | null;
}

export interface Navegador {
  ua: string;
  plataforma: string;
  nucleos: number | null;
  memoria_gb: number | null;
  tela: [number, number] | null;
  dpr: number | null;
  idioma: string | null;
}

export interface Relatorio {
  esquema: string;
  data: string;
  duracao_s: number;
  plataforma: Navegador & { so: string; arch: string; sicro: string; variaveis: Record<string, string | null> };
  motor: { nome: string; versao: string | null; webview: string | null };
  ffmpeg: string;
  /** Início de cada mídia de teste: sem ele o `tempo3` de outra máquina não se lê. */
  midias: { nome: string; p0_s: number; inicio_s: number }[];
  sonda: Record<string, unknown>;
  /** `oculta`: a página ficou oculta durante a etapa (rAF e vídeo param; os tempos não valem). */
  etapas: { medida: Medida; chave: string; ms: number; ok: boolean; oculta: boolean }[];
  tempo3: Record<string, unknown>;
  pool8: Record<string, unknown>;
  compose: Record<string, unknown>;
  switch: Record<string, unknown>;
  linha: Record<string, unknown>;
}

export interface Dependencias {
  /** Prepara as mídias no backend (`edicao_diagnostico_midias`); `aoAvancar` recebe [feitas, total]. */
  midias(aoAvancar: (feitas: number, total: number) => void): Promise<MidiasDiagnostico>;
  ambiente(m: MidiasDiagnostico): Ambiente;
  salvar(json: Relatorio): Promise<string>;
  navegador(): Navegador;
  agora(): Date;
  aoProgresso?(p: Progresso): void;
  plano?: Etapa[];
}

/** Nome e versão do motor pelo UA. WebView2 se apresenta como Chromium com `Edg/`. */
export function motorDoWebview(ua: string): { nome: string; versao: string | null } {
  let m: RegExpExecArray | null;
  if ((m = /Edg\/([\d.]+)/.exec(ua))) return { nome: "WebView2", versao: m[1] ?? null };
  if ((m = /(?:HeadlessChrome|Chrome|Chromium)\/([\d.]+)/.exec(ua))) return { nome: "Chromium", versao: m[1] ?? null };
  // o WebKitGTK congela o UA (605.1.15); a versão de verdade vem do backend (`webview`)
  if ((m = /AppleWebKit\/([\d.]+)/.exec(ua))) return { nome: /Linux|X11/.test(ua) ? "WebKitGTK" : "WebKit", versao: m[1] ?? null };
  return { nome: "desconhecido", versao: null };
}

/** ISO com o fuso local: 2026-11-14T03:12:45-03:00. */
export function isoLocal(d: Date): string {
  const p = (n: number, k = 2) => String(Math.abs(n)).padStart(k, "0");
  const fz = -d.getTimezoneOffset();
  return (
    `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}` +
    `${fz >= 0 ? "+" : "-"}${p(Math.trunc(fz / 60))}:${p(fz % 60)}`
  );
}

/** O que existe no webview e se o canvas fica limpo depois de desenhar um vídeo de teste. */
export async function sondar(amb: Ambiente): Promise<Record<string, unknown>> {
  const g = globalThis as unknown as Record<string, unknown>;
  const c = amb.canvas(4, 4);
  const ctx = c.getContext("2d", { willReadFrequently: true });
  const v = amb.video();
  const s: Record<string, unknown> = {
    rvfc: "requestVideoFrameCallback" in v,
    ctx_filter: !!ctx && "filter" in ctx,
    offscreen_canvas: typeof g.OffscreenCanvas !== "undefined",
    webcodecs: typeof g.VideoDecoder !== "undefined",
    video_no_palco: "2x2 px, opacidade 0.01",
  };
  try {
    v.muted = true;
    v.preload = "auto";
    v.src = amb.url("r0.mp4");
    s.carregou = await esperar(amb, v, "loadeddata", 8000);
    ctx?.drawImage(v, 0, 0, 4, 4);
    try {
      ctx?.getImageData(0, 0, 1, 1);
      s.canvas_manchado = false;
    } catch (e) {
      s.canvas_manchado = true;
      s.erro_leitura = String(e);
    }
    s.cross_origin = v.crossOrigin;
  } catch (e) {
    if (e instanceof Cancelado) throw e;
    s.erro = textoDeErro(e);
  } finally {
    v.pause();
    v.remove();
  }
  return s;
}

/** `caminho` null = o salvar falhou (`erro`); o relatório continua na tela para "Copiar". */
export interface Resultado {
  relatorio: Relatorio;
  caminho: string | null;
  erro?: string;
}

/** Um clique: mídias → sonda → medições → salvar. Devolve null se cancelado (nada é gravado). */
export async function diagnosticar(d: Dependencias): Promise<Resultado | null> {
  const plano = d.plano ?? PLANO;
  const inicio = d.agora();
  const p: Progresso = {
    estados: { midias: "rodando", tempo3: "espera", pool8: "espera", compose: "espera", switch: "espera", linha: "espera" },
    feitas: 0,
    total: plano.length,
    midias: null,
  };
  const avisa = () => d.aoProgresso?.({ ...p, estados: { ...p.estados } });
  avisa();
  let m: MidiasDiagnostico;
  try {
    m = await d.midias((feitas, total) => {
      p.midias = [feitas, total];
      avisa();
    });
  } catch (e) {
    p.estados.midias = "falhou";
    avisa();
    throw e;
  }
  p.estados.midias = "feita";
  avisa();
  const amb = d.ambiente(m);
  const nav = d.navegador();
  try {
    const sonda = await sondar(amb);
    const rel: Relatorio = {
      esquema: ESQUEMA_DIAGNOSTICO,
      data: isoLocal(inicio),
      duracao_s: 0,
      plataforma: { ...nav, so: m.ambiente.so, arch: m.ambiente.arch, sicro: m.ambiente.sicro, variaveis: m.ambiente.variaveis },
      motor: { ...motorDoWebview(nav.ua), webview: m.ambiente.webview },
      ffmpeg: m.ffmpeg,
      midias: m.midias.map(({ nome, p0_s, inicio_s }) => ({ nome, p0_s, inicio_s })),
      sonda,
      etapas: [],
      tempo3: {},
      pool8: {},
      compose: {},
      switch: {},
      linha: {},
    };
    for (const e of plano) {
      if (p.estados[e.medida] === "espera") p.estados[e.medida] = "rodando";
      avisa();
      amb.oculta?.();
      const t0 = amb.agora();
      let ok = true;
      try {
        rel[e.medida][e.chave] = await e.rodar(amb, m);
      } catch (err) {
        if (err instanceof Cancelado) throw err;
        ok = false;
        rel[e.medida][e.chave] = { erro: textoDeErro(err) };
      }
      rel.etapas.push({ medida: e.medida, chave: e.chave, ms: Math.round(amb.agora() - t0), ok, oculta: amb.oculta?.() ?? false });
      p.feitas++;
      const restam = plano.slice(p.feitas).some((x) => x.medida === e.medida);
      const falhou = rel.etapas.some((x) => x.medida === e.medida && !x.ok);
      if (falhou) p.estados[e.medida] = "falhou";
      else if (!restam) p.estados[e.medida] = "feita";
      avisa();
    }
    rel.duracao_s = Math.round((d.agora().getTime() - inicio.getTime()) / 100) / 10;
    try {
      return { relatorio: rel, caminho: await d.salvar(rel) };
    } catch (e) {
      return { relatorio: rel, caminho: null, erro: toSicroError(e).message };
    }
  } catch (e) {
    if (e instanceof Cancelado) return null;
    throw e;
  }
}
