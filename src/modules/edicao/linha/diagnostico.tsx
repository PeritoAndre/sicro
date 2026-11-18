/**
 * Medida da linha do tempo para o Diagnóstico (aceite do P4.1: 200 clipes com menos de 16 ms por quadro de tela,
 * medidos DENTRO do app: o WebKitGTK pinta a página por software). Monta a `VistaLinha` com um projeto sintético de
 * 200 clipes curtos (0,4 a 0,55 s, como a revisão mediu), com tabela de quadros, tiras e ondas feitas na hora (sem a
 * store: o projeto aberto não muda), e roda quatro fases, um passo por quadro de tela: rolar, zoom, cabeça andando e
 * arrasto de um texto. Em cada quadro: o trabalho (mudança + render do React + leiaute) e o intervalo
 * entre quadros (inclui a pintura). Dono: P4.1.
 */

import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { useState, type Dispatch, type SetStateAction } from "react";
import type { Caixa, ClipeAudio, Onda, PedidoTira, ProjetoEdicao, Selecao, TiraMiniaturas } from "@domain/edicao";
import { novoId, novoProjeto } from "../modelo/projeto";
import { resolver, tempoDoQuadro, type ContextoTempo } from "../modelo/tempo";
import { novoClipeAudio, novoDestaque, novoRastreio, novaTrilhaNarracao } from "../inspetor/novos";
import type { Ambiente } from "../diagnostico/base";
import { VistaLinha, type AcoesLinha, type FonteCabeca, type UiDaLinha } from "./LinhaDoTempo";
import { UI_PADRAO } from "./estado";
import { midiaSintetica, tabelaSintetica, textoLivre, trechoNosQuadros, trilhaS } from "./testeLinha";
import { tirasDeMentira, WS_DE_MENTIRA } from "./tiras";
import { ondasDeMentira } from "./ondas";

/** Limite do aceite (ms de trabalho por quadro de tela). */
export const LIMITE_MS = 16;
export const N_CLIPES = 200;

export interface FaseLinha {
  quadros: number;
  /** Só o JS da mudança (sem o leiaute), mediana (ms). */
  js_p50: number;
  /** Mudança + render + leiaute, por quadro (ms). */
  trabalho_p50: number;
  trabalho_p95: number;
  trabalho_max: number;
  /** Entre dois quadros de tela (ms; inclui a pintura do anterior). */
  intervalo_p50: number;
  intervalo_p95: number;
}

export interface ResLinha {
  clipes: number;
  fases: Record<"rolar" | "zoom" | "cabeca" | "arrastar", FaseLinha>;
  /** Quadros de tela logo depois do zoom parar (a árvore assenta numa transição repartida): só o intervalo. */
  assentar: Pick<FaseLinha, "quadros" | "intervalo_p50" | "intervalo_p95"> & { intervalo_max: number };
  ok: boolean;
}

/** Projeto de 200 clipes curtos (12 a 16 quadros a 30 qps) com tabela, um destaque a cada 3, 40 textos de 0,3 s em
 *  duas trilhas, 20 narrações de 1 s (o da revisão da rodada 2). */
export function projetoDe200(): { doc: ProjetoEdicao; ctx: ContextoTempo } {
  const doc = novoProjeto("diagnostico");
  doc.saida.fps = { num: 30, den: 1 };
  const tab = tabelaSintetica(3000);
  const tabA = tabelaSintetica(300);
  doc.midias.push(midiaSintetica("m1", tab, { sha256: "diagnostico-linha-m1".padEnd(64, "0") }));
  doc.midias.push({ ...midiaSintetica("a1", tabA, { sha256: "diagnostico-linha-a1".padEnd(64, "0") }), tipo: "audio" });
  const r = novoRastreio("m1", 0, { x: 0.4, y: 0.4, w: 0.1, h: 0.2 } as Caixa);
  r.nome = "PP1";
  doc.rastreios.push(r);
  for (let i = 0; i < N_CLIPES; i++) {
    const a = (i * 14) % 2900;
    const c = trechoNosQuadros("m1", tab, a, a + 12 + (i % 5));
    c.rotulo = `Cena ${i + 1}`;
    if (i % 3 === 0) c.efeitos.push({ ...novoDestaque(r.id), ativo: { de: tempoDoQuadro(tab, a + 2), ate: tempoDoQuadro(tab, a + 8) } });
    doc.principal.push(c);
  }
  const ids = doc.principal.map((c) => (c as { id: string }).id);
  const s1 = trilhaS([]);
  const s2 = trilhaS([]);
  for (let i = 0; i < 40; i++) (i % 2 ? s2 : s1).clipes.push(textoLivre({ ref: "clipe", clipe: ids[i * 5]!, desloc: 0.1 }, 0.3, `Texto ${i + 1}`));
  doc.sobreposicoes.push(s1, s2);
  const a = novaTrilhaNarracao();
  for (let i = 0; i < 20; i++) a.clipes.push({ ...novoClipeAudio("a1", `Narração ${i + 1}`, { ref: "clipe", clipe: ids[i * 10]!, desloc: 0 }), ate_s: 1 } as ClipeAudio);
  doc.audios.push(a);
  for (let i = 0; i < 12; i++) doc.marcadores.push({ id: novoId(), rotulo: "", cor: "#5aa9e6", pos: { ref: "programa", t: i * 8 } });
  return { doc, ctx: { tabela: (id) => (id === "m1" ? tab : id === "a1" ? tabA : null) } };
}

/** Imagem de 64 células (a mesma para toda tira da medida). */
function spriteDeMentira(w: number, h: number): string {
  const c = document.createElement("canvas");
  c.width = w * 64;
  c.height = h;
  const g = c.getContext?.("2d");
  if (!g) return "data:,";
  for (let k = 0; k < 64; k++) {
    g.fillStyle = `hsl(${(k * 37) % 360} 25% 28%)`;
    g.fillRect(k * w, 0, w, h);
    g.fillStyle = "#cfd6df";
    g.fillRect(k * w + w * 0.3, h * 0.4, w * 0.2, h * 0.3);
  }
  return c.toDataURL("image/jpeg", 0.8);
}

/** Tiras e ondas feitas na hora (a forma das do app: `n` pela largura pedida, 100 picos por segundo). */
function mentiras(): () => void {
  const sprites = new Map<string, string>();
  tirasDeMentira({
    pedir: (_ws, p: PedidoTira) => {
      const h = p.altura;
      const w = Math.max(2, Math.round((h * 16) / 9 / 2) * 2);
      const n = Math.max(1, Math.min(256, Math.round((p.largura ?? w) / w)));
      const tira: TiraMiniaturas = { relativo: `diagnostico/${w}x${h}.jpg`, w, h, n, de_s: p.de_s, ate_s: p.ate_s, celulas: [], miniaturas: Array.from({ length: n }, (_, k) => k % 64) };
      return Promise.resolve({ chave: `tira:${p.sha256}`, tira });
    },
    url: (t) => {
      const k = `${t.w}x${t.h}`;
      let u = sprites.get(k);
      if (!u) sprites.set(k, (u = spriteDeMentira(t.w, t.h)));
      return u;
    },
  });
  ondasDeMentira((sha): Onda => {
    const n = 100 * 120;
    const min: number[] = new Array(n);
    const max: number[] = new Array(n);
    let x = sha.charCodeAt(12) || 7;
    for (let i = 0; i < n; i++) {
      x = (x * 9301 + 49297) % 233280;
      const v = ((x / 233280) * 0.55 + 0.18 * Math.abs(Math.sin(i / 37))) * 20000;
      min[i] = -Math.round(v);
      max[i] = Math.round(v * 0.9);
    }
    return { inicio_s: 0, por_s: 100, min, max };
  });
  return () => {
    tirasDeMentira(null);
    ondasDeMentira(null);
  };
}

interface Controle {
  ui: Dispatch<SetStateAction<UiDaLinha>>;
  selecao: Dispatch<SetStateAction<Selecao>>;
  cabeca: { q: number; avisar: (() => void) | null };
}

function Bancada({ doc, ctx, controle }: { doc: ProjetoEdicao; ctx: ContextoTempo; controle: Controle }) {
  const [ui, setUi] = useState<UiDaLinha>({ ...UI_PADRAO, alturas: {}, travadas: {} });
  const [selecao, setSelecao] = useState<Selecao>({ itens: [], foco: null, efeito: null });
  controle.ui = setUi;
  controle.selecao = setSelecao;
  const [linha] = useState(() => resolver(doc, ctx));
  const [cabeca] = useState<FonteCabeca>(() => ({
    ler: () => ({ q: controle.cabeca.q, fora: false }),
    assinar: (f) => {
      controle.cabeca.avisar = f;
      return () => void (controle.cabeca.avisar = null);
    },
  }));
  const [acoes] = useState<AcoesLinha>(() => ({ selecionar: setSelecao, posicionar() {}, alterar() {}, dividir() {}, abrirEntrada() {}, tocar() {} }));
  return (
    <VistaLinha
      doc={doc}
      linha={linha}
      ctx={ctx}
      selecao={selecao}
      soLeitura={false}
      problemas={[]}
      ws={WS_DE_MENTIRA}
      raizCache={WS_DE_MENTIRA}
      ui={ui}
      definirUi={(p) => setUi((u) => ({ ...u, ...p }))}
      cabeca={cabeca}
      acoes={acoes}
      atalhos={false}
    />
  );
}

const ordenar = (a: number[]) => [...a].sort((x, y) => x - y);
const pct = (s: number[], p: number) => (s.length ? (s[Math.min(s.length - 1, Math.floor(p * (s.length - 1)))] as number) : 0);
const dois = (v: number) => Math.round(v * 100) / 100;

/** Roda as fases em `amb` (o palco do Diagnóstico) e devolve os tempos. */
export async function medirLinha(amb: Pick<Ambiente, "agora" | "quadro" | "palco">, op: { quadros?: number } = {}): Promise<ResLinha> {
  const n = op.quadros ?? 120;
  const hospedeiro = amb.palco.parentElement ?? document.body;
  const caixa = document.createElement("div");
  caixa.style.cssText = "position:absolute;left:0;top:0;width:1280px;height:320px;overflow:hidden;z-index:1";
  hospedeiro.appendChild(caixa);
  const raiz = createRoot(caixa);
  const { doc, ctx } = projetoDe200();
  const controle: Controle = { ui: () => {}, selecao: () => {}, cabeca: { q: 0, avisar: null } };
  const voltar = mentiras();
  try {
    flushSync(() => raiz.render(<Bancada doc={doc} ctx={ctx} controle={controle} />));
    // as tiras pedem depois de a tela parar; chegam antes das fases
    await new Promise((r) => setTimeout(r, 400));
    for (let k = 0; k < 3; k++) await amb.quadro();
    const rolo = caixa.querySelector<HTMLElement>("[data-linha-rolo]");
    if (!rolo) throw new Error("linha não montou");
    /** `diferido`: o painel faz o trabalho no quadro seguinte (arrasto, cabeça): conta também o que rodou antes desta
     *  chamada naquele quadro. */
    const js: number[] = [];
    const fase = async (passo: (k: number) => void, diferido = false): Promise<FaseLinha> => {
      js.length = 0;
      const trabalho: number[] = [];
      const intervalo: number[] = [];
      let antes = await amb.quadro();
      for (let k = 0; k < n; k++) {
        const t0 = amb.agora();
        flushSync(() => passo(k));
        const t1 = amb.agora();
        void caixa.offsetHeight; // leiaute agora (entra na conta)
        let w = amb.agora() - t0;
        js.push(t1 - t0);
        const t = await amb.quadro();
        if (diferido) {
          void caixa.offsetHeight;
          w += amb.agora() - t;
        }
        trabalho.push(w);
        intervalo.push(t - antes);
        antes = t;
      }
      const tr = ordenar(trabalho);
      const so = ordenar(js);
      const iv = ordenar(intervalo);
      return { quadros: n, js_p50: dois(pct(so, 0.5)), trabalho_p50: dois(pct(tr, 0.5)), trabalho_p95: dois(pct(tr, 0.95)), trabalho_max: dois(tr[tr.length - 1] ?? 0), intervalo_p50: dois(pct(iv, 0.5)), intervalo_p95: dois(pct(iv, 0.95)) };
    };
    const largura = () => rolo.scrollWidth - rolo.clientWidth;
    const rolar = await fase((k) => {
      rolo.scrollLeft = (k * 97) % Math.max(1, largura());
      rolo.dispatchEvent(new Event("scroll"));
    });
    // zoom pela roda com Ctrl (o caminho do perito): metade aproxima, metade afasta, no meio da tela
    const rr = rolo.getBoundingClientRect();
    const zoom = await fase((k) => {
      rolo.dispatchEvent(new WheelEvent("wheel", { bubbles: true, cancelable: true, ctrlKey: true, deltaY: k < n / 2 ? -25 : 25, clientX: rr.left + rr.width / 2, clientY: rr.top + 40 }));
    });
    const iv: number[] = [];
    let antes = await amb.quadro();
    for (let k = 0; k < 40; k++) {
      const t = await amb.quadro();
      iv.push(t - antes);
      antes = t;
    }
    const ivs = ordenar(iv);
    const assentar = { quadros: iv.length, intervalo_p50: dois(pct(ivs, 0.5)), intervalo_p95: dois(pct(ivs, 0.95)), intervalo_max: dois(ivs[ivs.length - 1] ?? 0) };
    const cabeca = await fase((k) => {
      controle.cabeca.q = k * 3;
      controle.cabeca.avisar?.();
    }, true);
    // arrasto de um texto: aperta, anda um pouco por quadro
    const alvo = caixa.querySelector<HTMLElement>('[data-k="livre"]');
    const r = alvo?.getBoundingClientRect();
    const x0 = r ? r.left + r.width / 2 : 0;
    const y0 = r ? r.top + r.height / 2 : 0;
    const ev = (tipo: string, x: number) => new PointerEvent(tipo, { bubbles: true, cancelable: true, clientX: x, clientY: y0, button: 0, pointerId: 7 });
    alvo?.dispatchEvent(ev("pointerdown", x0));
    const arrastar = await fase((k) => {
      rolo.dispatchEvent(ev("pointermove", x0 + 4 + k * 2));
    }, true);
    rolo.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true, pointerId: 7 }));
    const fases = { rolar, zoom, cabeca, arrastar };
    return { clipes: N_CLIPES, fases, assentar, ok: Object.values(fases).every((f) => f.trabalho_p95 < LIMITE_MS) };
  } finally {
    raiz.unmount();
    caixa.remove();
    voltar();
  }
}

/** A etapa para o plano do Diagnóstico (`diagnostico/executar.ts::PLANO`; o integrador registra a medida "linha"). */
export const ETAPA_LINHA = { medida: "linha", chave: "200", rodar: (amb: Ambiente) => medirLinha(amb) } as const;
