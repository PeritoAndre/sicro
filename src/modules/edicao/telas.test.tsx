/**
 * Telas da Edição pelo padrão do app (renderToStaticMarkup, sem DOM): rótulos exatos do §8.6, nenhum texto de
 * exemplo, problema no cartão, item desconhecido apagado; e as regras de código do palco.
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiaRef, Problema, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";

const cmd = vi.hoisted(() => ({ edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn(), edicaoMidiaInfo: vi.fn(), edicaoTabelaQuadros: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useEdicaoStore } from "./store/edicaoStore";
import { novoProjeto, novoTrecho } from "./modelo/projeto";
import { tempoDoQuadro } from "./modelo/tempo";
import { useRecursos } from "./palco/recursos";
import { Transporte, type TransporteProps } from "./palco/Transporte";
import { CartaoClipe, type CartaoProps } from "./sequencia/CartaoClipe";
import { VistaSequencia } from "./sequencia/Sequencia";
import { recursoDe } from "./palco/recursos";
import { resolver } from "./modelo/tempo";
import { acharMidia } from "./modelo/projeto";
import { COLUNAS_PADRAO, efetivas, gradeDasColunas, lerColunas } from "./colunas";

/** Textos visíveis (nós de texto e `title`). */
function textos(html: string): string[] {
  const t = html
    .replace(/<svg[\s\S]*?<\/svg>/g, " ")
    .replace(/<[^>]+>/g, "\n")
    .split("\n")
    .map((s) => s.replace(/&amp;/g, "&").trim())
    .filter(Boolean);
  const titles = [...html.matchAll(/ title="([^"]*)"/g)].map((m) => m[1]!);
  return [...t, ...titles];
}

/** Rótulos do §8.6 que estas telas usam. */
const ROTULOS = new Set([
  "Reproduzir",
  "Quadro anterior",
  "Próximo quadro",
  "Início do clipe",
  "Fim do clipe",
  "Quadro final",
  "Prévia do render",
  "Ajustar à tela",
  "Expandir",
  "Trecho…",
  "Dos eventos…",
  "Título…",
  "Congelar aqui",
  "Imagem…",
  "Trecho",
  "Título",
  "Quadro congelado",
  "Imagem",
  "Tela dividida",
  "Taxa variável",
  "Vídeo ausente",
  "Danificado",
  "Ver",
  "Duplicar",
  "Remover",
  "Adicionar",
  "Terço inferior",
  "Narração",
]);

const transporte = (p: Partial<TransporteProps> = {}): TransporteProps => ({
  de: 0,
  ate: 20,
  faixa: [2, 10],
  cursor: 12.345,
  quadro: 371,
  base: "fonte",
  tocando: false,
  bordas: true,
  fiel: "viva",
  render: "nada",
  expandido: false,
  onBuscar() {},
  onBorda() {},
  onTocar() {},
  onPasso() {},
  onInicio() {},
  onFim() {},
  onQuadroFinal() {},
  onPreviaRender() {},
  onAjustar() {},
  onExpandir() {},
  ...p,
});

describe("Transporte", () => {
  it("tempo HH:MM:SS,mmm e o quadro da tabela; só os rótulos do §8.6", () => {
    const html = renderToStaticMarkup(createElement(Transporte, transporte()));
    const ts = textos(html);
    expect(ts).toContain("00:00:12,345");
    expect(ts).toContain("quadro 371");
    for (const t of ts.filter((x) => x !== "00:00:12,345" && x !== "quadro 371")) expect(ROTULOS.has(t), t).toBe(true);
    for (const r of ["Reproduzir", "Quadro anterior", "Próximo quadro", "Início do clipe", "Fim do clipe", "Quadro final", "Prévia do render", "Ajustar à tela", "Expandir"]) expect(ts).toContain(r);
    expect(html).toContain('data-borda="inicio"');
    expect(html).not.toMatch(/placeholder/i);
  });

  it("bordas desligadas (mídia sem tempo confiável): sem alças de borda", () => {
    const html = renderToStaticMarkup(createElement(Transporte, transporte({ bordas: false })));
    expect(html).not.toContain("data-borda");
    expect(html).toContain("data-faixa");
  });
});

const cartao = (p: Partial<CartaoProps> = {}): CartaoProps => ({
  indice: 0,
  tipo: "Trecho",
  tipoCru: "trecho",
  nome: "cam1",
  duracao: 12,
  selos: [],
  problemas: [],
  presos: [],
  selecionado: false,
  desligado: false,
  arrastando: false,
  soltar: null,
  soLeitura: false,
  onSelecionar() {},
  onVer() {},
  onDuplicar() {},
  onRemover() {},
  onAlca() {},
  onAdicionar() {},
  onPreso() {},
  ...p,
});
const prob = (nivel: Problema["nivel"], mensagem: string): Problema => ({ nivel, codigo: "x", mensagem, clipe: "c", efeito: null, trilha: null, midia: null, campo: null, n: null });

describe("CartaoClipe", () => {
  it("número, tipo, nome, duração e selos; erro numa linha curta (o erro antes do aviso)", () => {
    const html = renderToStaticMarkup(createElement(CartaoClipe, cartao({ selos: ["Taxa variável", { velocidade: "0,25×" }], problemas: [prob("aviso", "Aviso A"), prob("erro", "Mídia ausente")] })));
    const ts = textos(html);
    expect(ts.slice(0, 5)).toEqual(["1", "Trecho", "cam1", "0:12", "Taxa variável"]);
    expect(ts).toContain("0,25×");
    expect(ts).toContain("Mídia ausente");
    expect(ts).not.toContain("Aviso A");
    expect(html).toContain('data-problema="x"');
  });

  it("selecionado mostra 'Adicionar'; desconhecido fica apagado com o tipo cru e sem Ver nem Duplicar", () => {
    expect(textos(renderToStaticMarkup(createElement(CartaoClipe, cartao({ selecionado: true }))))).toContain("Adicionar");
    const html = renderToStaticMarkup(createElement(CartaoClipe, cartao({ tipo: null, tipoCru: "do_futuro", nome: "", duracao: null })));
    const ts = textos(html);
    expect(ts).toContain("do_futuro");
    expect(ts).not.toContain("Ver");
    expect(ts).not.toContain("Duplicar");
    expect(ts).toContain("Remover");
    expect(textos(renderToStaticMarkup(createElement(CartaoClipe, cartao({ soLeitura: true }))))).not.toContain("Remover");
  });
});

// O SSR do zustand lê o estado inicial da store: a lista é conferida pela vista, com os dados da store aberta.
describe("Sequência com o projeto aberto", () => {
  const WS = "/casos/telas";
  const sha = "d".repeat(64);
  const tab: TabelaQuadros = { tb: { num: 1, den: 30 }, pts0: 0, passos: Array(299).fill(1), passo_mediano: 1, chaves: [0], vfr: true, origem: "decodificador" };
  beforeEach(async () => {
    vi.useFakeTimers({ now: new Date("2026-11-14T05:00:00Z") });
    for (const f of Object.values(cmd)) f.mockReset();
    cmd.edicaoSalvar.mockResolvedValue({ updated_at: "" });
    cmd.edicaoAnalisar.mockResolvedValue({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null });
    const p: ProjetoEdicao = novoProjeto();
    p.midias = [{ id: "m", sha256: sha, tipo: "video", nome: "cam1.mp4", relativo: "cam1.mp4", vfr: true, ocultacoes: [] } as unknown as MidiaRef];
    p.principal = [{ ...novoTrecho("m", 1, 5), id: "a" }, { tipo: "do_futuro", id: "f" }, { ...novoTrecho("m", 6, 9), id: "b", velocidade: { tipo: "constante", fator: 0.5 } }];
    cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache" });
    await useEdicaoStore.getState().abrir(WS, p.id);
    useRecursos.setState({ midias: { [sha]: { info: null, tabela: tab, erro: null, pedindo: false } } });
  });
  afterEach(async () => {
    await useEdicaoStore.getState().fechar({ descartar: true }).catch(() => {});
    vi.useRealTimers();
  });

  it("barra de botões, um cartão por item, desconhecido apagado, duração pela tabela", () => {
    const st = useEdicaoStore.getState();
    const doc = st.doc!;
    const linha = resolver(doc, { tabela: (id) => recursoDe(acharMidia(doc, id)?.sha256).tabela });
    const nada = () => {};
    const html = renderToStaticMarkup(
      createElement(VistaSequencia, {
        doc,
        analise: st.analise,
        avisos: st.avisos,
        focoId: null,
        soLeitura: false,
        linha,
        podeCongelar: false,
        recurso: recursoDe,
        arrasto: null,
        onForm: nada,
        onCongelar: nada,
        onImagem: nada,
        onSelecionar: nada,
        onDuplicar: nada,
        onRemover: nada,
        onAlca: () => nada,
        onAdicionar: nada,
        onPreso: nada,
      }),
    );
    const ts = textos(html);
    for (const r of ["Trecho…", "Título…", "Congelar aqui", "Imagem…", "Dos eventos…"]) expect(ts).toContain(r);
    expect(html.match(/data-cartao="/g)).toHaveLength(3);
    expect(ts).toContain("do_futuro");
    expect(ts).toContain("0:04");
    expect(ts).toContain("0:06");
    expect(ts).toContain("0,5×");
    for (const t of ts.filter((x) => !/^\d|^0:|×$/.test(x) && x !== "cam1" && x !== "do_futuro")) expect(ROTULOS.has(t), t).toBe(true);
    expect(html).not.toMatch(/placeholder/i);
    expect(tempoDoQuadro(tab, 30)).toBe(1);
  });

});

describe("colunas", () => {
  it("largura guardada dentro dos limites; recolher e expandir", () => {
    expect(lerColunas(null)).toEqual(COLUNAS_PADRAO);
    expect(lerColunas('{"seq": 9000, "insp": 10, "seqFechada": true}')).toEqual({ seq: 480, insp: 280, seqFechada: true, inspFechada: false });
    expect(lerColunas("lixo")).toEqual(COLUNAS_PADRAO);
    expect(gradeDasColunas(COLUNAS_PADRAO, false)).toBe("300px 4px minmax(0, 1fr) 4px 340px");
    expect(gradeDasColunas({ ...COLUNAS_PADRAO, seqFechada: true }, false)).toBe("0px 0px minmax(0, 1fr) 4px 340px");
    expect(gradeDasColunas(COLUNAS_PADRAO, true)).toBe("0px 0px minmax(0, 1fr) 0px 0px");
    // janela mínima (868 px úteis): as laterais cedem até o mínimo; com espaço, valem as guardadas
    expect(efetivas(COLUNAS_PADRAO, 868)).toEqual({ seq: 220, insp: 280 });
    expect(efetivas(COLUNAS_PADRAO, 1208)).toEqual({ seq: 300, insp: 340 });
    expect(gradeDasColunas({ ...COLUNAS_PADRAO, seq: 480 }, false, 1134)).toBe("284px 4px minmax(0, 1fr) 4px 340px");
  });
});

describe("regras de código da Edição", () => {
  const raiz = fileURLToPath(new URL(".", import.meta.url));
  const arquivos: string[] = [];
  const andar = (d: string) => {
    for (const n of readdirSync(d)) {
      const p = join(d, n);
      if (statSync(p).isDirectory()) {
        // o diagnóstico (P0.9) mede o próprio elemento de propósito
        if (n !== "diagnostico") andar(p);
      } else if (/\.tsx?$/.test(n) && !/\.test\.tsx?$/.test(n)) arquivos.push(p);
    }
  };
  andar(raiz);

  it("tempo e tamanho do <video> só pela porta (relogioMidia + motor/elemento); nunca requestVideoFrameCallback", () => {
    const porta = /palco[\\/]motor[\\/](relogioMidia|elemento)\.ts$/;
    const proibido = /\.(currentTime|duration|videoWidth|videoHeight|seekable)\b|requestVideoFrameCallback/;
    const achados = arquivos.filter((f) => !porta.test(f)).flatMap((f) =>
      readFileSync(f, "utf8")
        .split("\n")
        .map((l, i) => [l, i] as const)
        .filter(([l]) => proibido.test(l) && !/^\s*(\/\/|\*|\/\*\*)/.test(l))
        .map(([l, i]) => `${f}:${i + 1}: ${l.trim()}`),
    );
    expect(achados).toEqual([]);
    expect(arquivos.length).toBeGreaterThan(20);
  });

  it("nenhum texto de exemplo em campo", () => {
    expect(arquivos.filter((f) => f.endsWith(".tsx") && /placeholder=/.test(readFileSync(f, "utf8")))).toEqual([]);
  });
});
