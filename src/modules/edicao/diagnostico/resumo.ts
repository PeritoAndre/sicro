/** Tabela curta do resultado: uma linha por variante, números principais de cada medida. */
import type { Relatorio } from "./executar";
import type { ResCompose } from "./compose";
import type { ResPool8 } from "./pool8";
import type { ResSwitch } from "./switch";
import { type ResTempo3, baseDeTempo } from "./tempo3";
import type { ResLinha } from "../linha/diagnostico";

/** O `LIMITE_MS` de `linha/diagnostico.tsx` (aceite do P4.1), sem carregar a linha aqui. */
export const LIMITE_LINHA_MS = 16;

export interface Valor { rotulo: string; valor: string; ok?: boolean }
export interface LinhaResumo { medida: string; variante: string; valores: Valor[]; erro?: string }

const VARIANTES: Record<string, string> = {
  fontes: "fontes",
  copias: "cópias",
  p720: "720p ×4",
  "1080p": "1080p ×4",
  p720_n2: "720p ×2",
  v1080_g30: "1080p → 720p",
  cam1_p720: "720p → 720p",
  padrao: "cam1 → cam2",
  mesmo_arquivo: "cam1 → cam1",
  velb_0_5: "0,5×",
  "200": "200 clipes",
};

const n0 = (v: number | null | undefined) => (v == null || !Number.isFinite(v) ? "—" : String(Math.round(v)));
const faixa = (s: { min: number; max: number } | null | undefined) => (s ? `${n0(s.min)}–${n0(s.max)}` : "—");

function tempo3(r: ResTempo3, chave: string, p0: (nome: string) => number | undefined): Valor[] {
  if (chave === "copias") {
    let erros = 0;
    let buscas = 0;
    // arquivo que não chegou a buscar conta as 11 buscas como erro
    for (const a of r.arquivos) {
      const k = a.previsto?.length ?? 11;
      buscas += k;
      erros += a.erros_previsto ?? k;
    }
    return [{ rotulo: "erros", valor: `${erros}/${buscas}`, ok: erros === 0 && buscas > 0 }];
  }
  const conta = { absoluto: 0, rebaseado: 0, outro: 0, nada: 0 };
  for (const a of r.arquivos) conta[baseDeTempo(a, p0(a.arquivo) ?? 0) ?? "nada"]++;
  const v: Valor[] = [
    { rotulo: "absoluto", valor: String(conta.absoluto) },
    { rotulo: "rebaseado", valor: String(conta.rebaseado) },
    { rotulo: "outro", valor: String(conta.outro) },
  ];
  if (conta.nada) v.push({ rotulo: "sem leitura", valor: String(conta.nada) });
  return v;
}

function pool8(r: ResPool8): Valor[] {
  const certos = r.ciclos.reduce((s, c) => s + c.quadro_entrada_lido.filter((q) => q === c.quadro_entrada_esperado).length, 0);
  const total = r.ciclos.reduce((s, c) => s + c.quadro_entrada_lido.length, 0);
  return [
    { rotulo: "fps", valor: n0(r.base?.fps) },
    { rotulo: "ms", valor: n0(r.base?.custo_js_ms?.p50) },
    { rotulo: "preparo", valor: faixa(r.ciclos.length ? { min: Math.min(...r.ciclos.map((c) => c.preparo_ms)), max: Math.max(...r.ciclos.map((c) => c.preparo_ms)) } : null) },
    { rotulo: "entrada", valor: `${certos}/${total}`, ok: total > 0 && certos === total },
  ];
}

function compose(r: ResCompose): Valor[] {
  return [
    { rotulo: "fps", valor: n0(r.completo.fps_raf) },
    { rotulo: "ms", valor: n0(r.completo.custo_js_ms?.p50) },
    { rotulo: "um desenho", valor: `${n0(r.um_desenho_sem_lupa.fps_raf)} · ${n0(r.um_desenho_sem_lupa.custo_js_ms?.p50)}` },
    { rotulo: "rVFC", valor: n0(r.completo.video[0]?.rvfc) },
  ];
}

function troca(r: ResSwitch): Valor[] {
  const certos = r.trocas.filter((t) => t.quadro_entrada_lido === t.quadro_entrada_esperado).length;
  return [
    { rotulo: "andou", valor: faixa(r.resumo?.tempo_andou_ms) },
    { rotulo: "preparo", valor: faixa(r.resumo?.preparo_do_proximo_ms) },
    { rotulo: "entrada", valor: `${certos}/${r.trocas.length}`, ok: r.trocas.length > 0 && certos === r.trocas.length },
  ];
}

/** Linha do tempo: o p95 do trabalho por quadro de cada fase, certo abaixo de `LIMITE_LINHA_MS`. */
function linha(r: ResLinha): Valor[] {
  const nomes: Record<keyof ResLinha["fases"], string> = { rolar: "rolar", zoom: "zoom", cabeca: "cabeça", arrastar: "arrastar" };
  return (Object.keys(nomes) as (keyof ResLinha["fases"])[]).map((k) => ({ rotulo: nomes[k], valor: n0(r.fases[k]?.trabalho_p95), ok: (r.fases[k]?.trabalho_p95 ?? Infinity) < LIMITE_LINHA_MS }));
}

export function resumir(rel: Relatorio, p0: (nome: string) => number | undefined): LinhaResumo[] {
  const linhas: LinhaResumo[] = [];
  for (const e of rel.etapas) {
    const r = rel[e.medida][e.chave] as Record<string, unknown> | undefined;
    const base = { medida: e.medida, variante: VARIANTES[e.chave] ?? e.chave };
    if (!r || "erro" in r) {
      linhas.push({ ...base, valores: [], erro: String((r as { erro?: unknown } | undefined)?.erro ?? "").split("\n")[0] });
      continue;
    }
    const valores =
      e.medida === "tempo3"
        ? tempo3(r as unknown as ResTempo3, e.chave, p0)
        : e.medida === "pool8"
          ? pool8(r as unknown as ResPool8)
          : e.medida === "compose"
            ? compose(r as unknown as ResCompose)
            : e.medida === "linha"
              ? linha(r as unknown as ResLinha)
              : troca(r as unknown as ResSwitch);
    linhas.push({ ...base, valores });
  }
  return linhas;
}
