/**
 * Medida da linha do tempo para o Diagnóstico (aceite do P4.1: 200 clipes com menos de 16 ms por quadro de tela,
 * medidos DENTRO do app: o WebKitGTK pinta a página por software). Monta a `VistaLinha` com um projeto sintético de
 * 200 clipes (sem a store: o projeto aberto não muda), e roda quatro fases, um passo por quadro de tela: rolar, zoom,
 * cabeça andando e arrasto de um texto. Em cada quadro: o trabalho (mudança + render do React + leiaute) e o intervalo
 * entre quadros (inclui a pintura). Dono: P4.1.
 */

import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { useState, type Dispatch, type SetStateAction } from "react";
import type { Caixa, ProjetoEdicao, Selecao } from "@domain/edicao";
import { novoId, novoProjeto, novoTrecho } from "../modelo/projeto";
import { resolver, type ContextoTempo } from "../modelo/tempo";
import { novoClipeAudio, novoDestaque, novoRastreio, novoTitulo, novaTrilhaNarracao } from "../inspetor/novos";
import type { Ambiente } from "../diagnostico/base";
import { VistaLinha, type AcoesLinha, type FonteCabeca, type UiDaLinha } from "./LinhaDoTempo";
import { UI_PADRAO } from "./estado";
import { midiaSintetica, textoLivre, trilhaS } from "./testeLinha";

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

/** Projeto de 200 clipes (trechos e títulos alternados), 40 textos em duas trilhas, 20 narrações, 60 destaques. */
export function projetoDe200(): { doc: ProjetoEdicao; ctx: ContextoTempo } {
  const doc = novoProjeto("diagnostico");
  doc.saida.fps = { num: 30, den: 1 };
  doc.midias.push(midiaSintetica("m1", null, { duracao_s: 3600 }));
  doc.midias.push({ ...midiaSintetica("a1", null, { duracao_s: 6 }), tipo: "audio" });
  const r = novoRastreio("m1", 0, { x: 0.4, y: 0.4, w: 0.1, h: 0.2 } as Caixa);
  r.nome = "PP1";
  doc.rastreios.push(r);
  for (let i = 0; i < N_CLIPES; i++) {
    if (i % 4 === 3) {
      doc.principal.push(novoTitulo(2));
      continue;
    }
    const c = novoTrecho("m1", i * 12, i * 12 + 4 + (i % 5));
    c.rotulo = `Cena ${i + 1}`;
    if (i % 3 === 0) c.efeitos.push({ ...novoDestaque(r.id), ativo: { de: i * 12 + 1, ate: i * 12 + 3 } });
    doc.principal.push(c);
  }
  const ids = doc.principal.map((c) => (c as { id: string }).id);
  const s1 = trilhaS([]);
  const s2 = trilhaS([]);
  for (let i = 0; i < 40; i++) (i % 2 ? s2 : s1).clipes.push(textoLivre({ ref: "clipe", clipe: ids[i * 5]!, desloc: 0.5 }, 2, `Texto ${i + 1}`));
  doc.sobreposicoes.push(s1, s2);
  const a = novaTrilhaNarracao();
  for (let i = 0; i < 20; i++) a.clipes.push(novoClipeAudio("a1", `Narração ${i + 1}`, { ref: "clipe", clipe: ids[i * 10]!, desloc: 0 }));
  doc.audios.push(a);
  for (let i = 0; i < 12; i++) doc.marcadores.push({ id: novoId(), rotulo: "", cor: "#5aa9e6", pos: { ref: "programa", t: i * 60 } });
  return { doc, ctx: { tabela: () => null } };
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
      ws={null}
      raizCache={null}
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
  try {
    flushSync(() => raiz.render(<Bancada doc={doc} ctx={ctx} controle={controle} />));
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
  }
}

/** A etapa para o plano do Diagnóstico (`diagnostico/executar.ts::PLANO`; o integrador registra a medida "linha"). */
export const ETAPA_LINHA = { medida: "linha", chave: "200", rodar: (amb: Ambiente) => medirLinha(amb) } as const;
