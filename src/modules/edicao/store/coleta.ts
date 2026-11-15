/**
 * Coletar trecho para a edição (§8.1): acrescenta trechos à edição corrente do caso sem abrir a tela.
 * Com o projeto aberto na tela, entra pela store (um escritor só); fechado, lê, acrescenta e grava.
 * A edição corrente é criada se não houver (ou se a guardada sumiu ou só abre para leitura).
 */

import { commands } from "@core/commands";
import type { Derivacao, Id, MidiaInfo, MidiaRef, ProjetoEdicao, Pts, Seg } from "@domain/edicao";
import type { VideoMedia } from "@domain/video";
import { comoChegou, completar, garantirMidia, novoId, novoProjeto, novoTrecho } from "../modelo/projeto";
import { correnteDe, definirCorrente, emSerie, separarErro, useEdicaoStore } from "./edicaoStore";
import { provenienciaDaMidia } from "@modules/video/editor/proveniencia";

export interface TrechoColetado {
  sha256: string;
  inicio_s: Seg;
  fim_s: Seg;
  rotulo?: string;
}

interface Preparado {
  midia: MidiaRef;
  inicio_s: Seg;
  fim_s: Seg;
  inicio_pts: Pts | null;
  fim_pts: Pts | null;
  rotulo: string;
}

// ---- retrato da mídia do caso (como o importador do Rust: importar_v1.rs) ----

function probe(m: VideoMedia): { format?: { start_time?: unknown }; streams: { codec_type?: string }[] } {
  try {
    const j: unknown = JSON.parse(m.raw_probe_json);
    if (typeof j !== "object" || j === null) return { streams: [] };
    const { format, streams } = j as { format?: unknown; streams?: unknown };
    return {
      format: typeof format === "object" && format !== null ? (format as { start_time?: unknown }) : undefined,
      streams: Array.isArray(streams) ? streams.filter((x) => typeof x === "object" && x !== null) : [],
    };
  } catch {
    return { streams: [] };
  }
}

/** Retrato de `derived_from_hash` + `derivation_json`, seguindo a cadeia até a raiz. */
function derivacao(m: VideoMedia, lista: VideoMedia[]): Derivacao | null {
  const de = m.derived_from_hash;
  if (!de) return null;
  let j: Record<string, unknown> = {};
  try {
    const lido: unknown = JSON.parse(m.derivation_json ?? "{}");
    if (typeof lido === "object" && lido !== null) j = lido as Record<string, unknown>;
  } catch {
    /* sem detalhe */
  }
  const tipo: Derivacao["tipo"] =
    j.kind === "clip" ? "trecho" : j.kind === "estabilizado" ? "estabilizado" : j.kind === "lente_video" ? "lente" : "outro";
  const bruto = j.ffmpeg_command ?? j.comando ?? j.comandos;
  const comando = typeof bruto === "string" ? bruto : Array.isArray(bruto) ? bruto.map((x) => (typeof x === "string" ? x : JSON.stringify(x))).join(" ") : "";
  let raiz = de;
  for (let i = 0; i < 64; i++) {
    const x = lista.find((v) => v.sha256 === raiz)?.derived_from_hash;
    if (!x || x === raiz) break;
    raiz = x;
  }
  return { tipo, de_sha256: de, raiz_sha256: raiz, comando, com_perdas: !(tipo === "trecho" && j.mode === "copy") };
}

/** `MidiaRef` de um vídeo do caso. `info` (tabela, tamanho exibido) quando o backend já tem; `prov` = a proveniência
 *  gravada na análise (P1.10, `provenienciaDaMidia`). */
export function midiaDoCaso(m: VideoMedia, lista: VideoMedia[], info: MidiaInfo | null, prov: Pick<MidiaRef, "recebido" | "relogio_desvio_s" | "gravador"> | null = null): MidiaRef {
  const p = probe(m);
  const ini = Number(p.format?.start_time);
  return {
    id: novoId(),
    tipo: "video",
    origem: "caso",
    media_id: m.id,
    sha256: m.sha256,
    nome: m.filename,
    relativo: m.relative_path,
    duracao_s: m.duration_s,
    // start_time do contêiner como está (pode ser negativo ou época)
    inicio_s: p.format?.start_time != null && Number.isFinite(ini) ? ini : null,
    largura: info?.exibido.w ?? m.width,
    altura: info?.exibido.h ?? m.height,
    taxa: info?.taxa ?? null,
    vfr: info?.vfr ?? false,
    tem_audio: info?.tem_audio ?? p.streams.some((s) => s.codec_type === "audio"),
    cor: null,
    derivacao: derivacao(m, lista),
    recebido: prov?.recebido ?? { nativo: null, obs: "" },
    relogio_desvio_s: prov?.relogio_desvio_s ?? null,
    gravador: prov?.gravador ?? null,
    decodificacao: null,
    ocultacoes: [],
  };
}

async function preparar(ws: string, itens: TrechoColetado[]): Promise<Preparado[]> {
  const lista = await commands.listVideoMedia(ws);
  const midias = new Map<string, { ref: MidiaRef; info: MidiaInfo | null }>();
  const saida: Preparado[] = [];
  for (const it of itens) {
    let m = midias.get(it.sha256);
    if (!m) {
      const v = lista.find((x) => x.sha256 === it.sha256);
      if (!v) throw new Error("midia_ausente");
      const info = await commands.edicaoMidiaInfo(ws, it.sha256).catch(() => null);
      // erro de leitura sobe (o Ctrl+3 mostra): nunca vira "não informado"
      const prov = await provenienciaDaMidia(ws, v.id);
      m = { ref: midiaDoCaso(v, lista, info, prov), info };
      midias.set(it.sha256, m);
    }
    // quem prende é o Rust: pts do quadro exibido; sem resposta, fica para `Analise.prender`
    const pts = (t: Seg) =>
      commands
        .edicaoQuadroEm(ws, it.sha256, { t })
        .then((q) => q.pts)
        .catch(() => null);
    const inicio_pts = await pts(it.inicio_s);
    // fim no fim da mídia (ou sem a faixa da mídia): fim_pts nulo
    const fim_pts = m.info && it.fim_s < m.info.ate_s ? await pts(it.fim_s) : null;
    saida.push({ midia: m.ref, inicio_s: it.inicio_s, fim_s: it.fim_s, inicio_pts, fim_pts, rotulo: it.rotulo ?? "" });
  }
  return saida;
}

/** Acrescenta à principal; devolve a posição (1…) do último trecho acrescentado. */
function acrescentar(d: ProjetoEdicao, itens: Preparado[]): number {
  for (const it of itens) {
    const id = garantirMidia(d, it.midia);
    const c = novoTrecho(id, it.inicio_s, it.fim_s, it.rotulo);
    c.inicio_pts = it.inicio_pts;
    c.fim_pts = it.fim_pts;
    d.principal.push(c);
  }
  return d.principal.length;
}

let fila: Promise<unknown> = Promise.resolve();

/** `n` = posição do último trecho na principal ("Trecho 7"). Uma coleta por vez. */
export function receber(ws: string, itens: TrechoColetado[], projeto?: Id): Promise<{ projeto: Id; n: number }> {
  // sem trecho não há o que gravar (nem edição a criar)
  if (itens.length === 0) return Promise.reject(new Error("intervalo"));
  const p = fila.then(() => receberJa(ws, itens, projeto));
  fila = p.catch(() => {});
  return p;
}

async function receberJa(ws: string, itens: TrechoColetado[], projeto?: Id): Promise<{ projeto: Id; n: number }> {
  const prontos = await preparar(ws, itens);
  // na vez da store: a tela não abre nem fecha o projeto entre a leitura e a gravação daqui
  return emSerie(() => gravar(ws, prontos, projeto));
}

async function gravar(ws: string, prontos: Preparado[], projeto?: Id): Promise<{ projeto: Id; n: number }> {
  const alvo = projeto ?? correnteDe(ws);
  const st = useEdicaoStore.getState();
  if (alvo && st.aberta?.projetoId === alvo && st.ws === ws && !st.soLeitura) {
    let n = 0;
    st.alterar("Trecho", (d) => {
      n = acrescentar(d, prontos);
    });
    return { projeto: alvo, n };
  }
  let doc: ProjetoEdicao | null = null;
  if (alvo) {
    try {
      const r = await commands.edicaoAbrir(ws, alvo);
      if (!r.so_leitura) doc = r.projeto;
    } catch (e) {
      // projeto pedido pelo nome não troca por outro calado; o corrente só é trocado se sumiu
      if (projeto || separarErro(e).codigo !== "projeto_nao_encontrado") throw e;
    }
    if (!doc && projeto) throw new Error("so_leitura");
  }
  const base = doc ? completar(doc) : novoProjeto();
  // só as duas listas mudam; o resto vai como chegou
  const novo: ProjetoEdicao = { ...base, midias: [...base.midias], principal: [...base.principal] };
  const n = acrescentar(novo, prontos);
  await commands.edicaoSalvar(ws, doc ? comoChegou(novo, base, doc) : novo);
  definirCorrente(ws, novo.id);
  return { projeto: novo.id, n };
}
