/**
 * Pagination — plugin ProseMirror que insere "spacers" entre blocos
 * para empurrar conteúdo para a próxima página, gerando paginação
 * VISUAL real (estilo Word) sem precisar separar o contenteditable em
 * múltiplos hosts.
 *
 * F7.5 — Paginação REATIVA. O plugin state armazena `opts` (margens, gap,
 * pageHeight). Mudanças em runtime são feitas via `setPaginationOptions
 * (view, partialOpts)` que dispara um `setMeta(KEY, { type: "setOpts",
 * opts })`. O `apply` atualiza o state e dispara recompute, fazendo o
 * conteúdo se reorganizar automaticamente com novos spacer heights —
 * igual ao Word: aumentar margem TOP empurra texto para próxima página
 * se necessário.
 */

import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet, type EditorView } from "@tiptap/pm/view";
import { measureLineBoxes, posAtLineStart } from "./lineBoxes";

export interface PaginationOptions {
  pageHeightCm: number;
  marginTopCm: number;
  marginBottomCm: number;
  /** Margens horizontais — NÃO afetam a altura útil, mas mudam a LARGURA do
   *  texto (e portanto o nº de linhas/altura). Rastreadas só pra DISPARAR
   *  recompute quando o perito arrasta a régua horizontal (senão a paginação
   *  fica obsoleta após a re-quebra do texto → buraco no modo split). */
  marginLeftCm: number;
  marginRightCm: number;
  gapCm: number;
  enabled: boolean;
  /** Frente X — modo de paginação:
   *  - "block" (padrão, estável): empurra o bloco INTEIRO p/ a próxima página.
   *  - "split" (experimental): quebra o parágrafo no meio da página (estilo
   *    Word). No X1 ainda delega ao block; o algoritmo inline entra no X3. */
  mode: "block" | "split";
}

export const PX_PER_CM = 96 / 2.54;

interface PaginationState {
  decos: DecorationSet;
  opts: PaginationOptions;
}

export const PAGINATION_PLUGIN_KEY = new PluginKey<PaginationState>(
  "sicroPagination",
);

type Meta =
  | { type: "setOpts"; opts: Partial<PaginationOptions> }
  | { type: "setDecos"; decos: DecorationSet };

export const Pagination = Extension.create<PaginationOptions>({
  name: "pagination",

  addOptions() {
    return {
      pageHeightCm: 29.7,
      marginTopCm: 3,
      marginBottomCm: 2.5,
      marginLeftCm: 2,
      marginRightCm: 2,
      gapCm: 0.7,
      enabled: true,
      // Padrão: "split" (quebra de parágrafo no meio, estilo Word) — validado
      // pelo perito (Frente X). O toggle na tela permite voltar pra "block".
      mode: "split",
    };
  },

  addProseMirrorPlugins() {
    const ext = this;
    if (!ext.options.enabled) return [];
    return [createPaginationPlugin(ext.options)];
  },
});

function createPaginationPlugin(initialOpts: PaginationOptions): Plugin {
  return new Plugin<PaginationState>({
    key: PAGINATION_PLUGIN_KEY,
    state: {
      init: () => ({ decos: DecorationSet.empty, opts: { ...initialOpts } }),
      apply(tr, old) {
        const meta = tr.getMeta(PAGINATION_PLUGIN_KEY) as Meta | undefined;
        if (meta?.type === "setOpts") {
          return {
            decos: old.decos.map(tr.mapping, tr.doc),
            opts: { ...old.opts, ...meta.opts },
          };
        }
        if (meta?.type === "setDecos") {
          return { decos: meta.decos, opts: old.opts };
        }
        return {
          decos: old.decos.map(tr.mapping, tr.doc),
          opts: old.opts,
        };
      },
    },
    props: {
      decorations(state) {
        return PAGINATION_PLUGIN_KEY.getState(state)?.decos;
      },
    },
    view(view) {
      let raf: number | null = null;
      let scheduled = false;
      // Convergência do modo "split": uma edição/margem grande pode precisar de
      // >1 passada pra estabilizar (o block converge em 1; a digitação
      // "converge sozinha" pelo fluxo de teclas). Re-agendamos até
      // decorationSetsEqual bater, limitado pra nunca loopar (M15).
      const MAX_CONVERGE_PASSES = 3;
      let convergePasses = 0;

      const schedule = () => {
        if (scheduled) return;
        scheduled = true;
        raf = window.requestAnimationFrame(() => {
          scheduled = false;
          raf = null;
          try {
            const pState = PAGINATION_PLUGIN_KEY.getState(view.state);
            if (!pState || !pState.opts.enabled) return;
            // F7.18 — NÃO recomputar contra um DOM DESACOPLADO/colapsado. No modo
            // multipage (zoom out) o EditorPage desmonta o <EditorContent>, então
            // `view.dom` fica detached; mas o plugin continua vivo e recebe
            // gatilhos async da fórmula (sicro:math-rendered, document.fonts,
            // timers de settle). Medir um DOM detached dá offsetTop/Height = 0 →
            // 0 quebras → spacers ZERADOS → o EditorPage recontava pageCount=1 e a
            // multipage "piscava" e colapsava pra 1 página. Texto puro não dispara
            // esses gatilhos pós-zoom, por isso só quebrava com fórmula. Mantendo
            // as últimas decorations boas (computadas com o editor visível), os
            // clones da multipage continuam corretos.
            if (!view.dom.isConnected || view.dom.offsetHeight === 0) return;
            const next = computePaginationDecos(view, pState.opts);
            // Convergiu (nada a mudar): para o ciclo de re-agendamento e AVISA os
            // consumidores (paper-stack + multipage no EditorPage) que a
            // paginação ASSENTOU. Eles recontam as páginas neste estado FINAL —
            // em vez de pegar estados intermediários do burst async (remoção +
            // re-inserção de spacers ao longo de imagem/KaTeX/fontes/timers), que
            // viam momentaneamente 0 quebras → pageCount=1 → colapsava a
            // multipage e desalinhava os cartões.
            if (decorationSetsEqual(pState.decos, next)) {
              try {
                window.dispatchEvent(
                  new CustomEvent("sicro:pagination-settled"),
                );
              } catch {
                /* sem window (SSR/teste) — ok */
              }
              return;
            }
            view.dispatch(
              view.state.tr.setMeta(PAGINATION_PLUGIN_KEY, {
                type: "setDecos",
                decos: next,
              } satisfies Meta),
            );
            // Só no split: re-agenda pra outra passada (bounded) — o block é
            // estável em 1 passada e não precisa.
            if (
              pState.opts.mode === "split" &&
              convergePasses < MAX_CONVERGE_PASSES
            ) {
              convergePasses += 1;
              schedule();
            }
          } catch (err) {
            // eslint-disable-next-line no-console
            console.warn("[pagination] recompute failed", err);
          }
        });
      };

      // F7.11 — REMOVIDO o ResizeObserver. Ele disparava schedule cada
      // vez que o view.dom mudava de altura — incluindo quando NÓS
      // MESMOS inseríamos um spacer (que muda a altura). Isso criava
      // um loop: dispatch → DOM cresce → RO trigger → schedule →
      // recompute → talvez novo dispatch → loop.
      //
      // Cobrimos o cenário "imagem terminou de carregar" via o update
      // do PM (caso a imagem mude o doc via insertContent) ou via
      // re-trigger manual quando necessário. Para typing/paste/undo,
      // update() é suficiente.

      // F7.12 — IMAGEM QUE CARREGA DEPOIS DA MEDIÇÃO. Uma foto grande (ex.: o
      // croqui de drone) ainda não terminou de carregar quando a paginação
      // mede a página → é medida com altura ~0 → a figura atômica "cabe" no fim
      // da página e NÃO é empurrada → ao carregar, estica e VAZA a quebra,
      // cascateando erros nas páginas seguintes (bug reportado: só sumia com um
      // Enter manual, que forçava recálculo pós-load). O evento `load` é
      // DISCRETO (uma vez por imagem, ao concluir) — ao contrário do
      // ResizeObserver removido no F7.11, NÃO loopa com os nossos spacers
      // (spacer não é <img>). Ao assentar a imagem, zera a convergência e
      // re-agenda: a figura é reempurrada pra próxima página corretamente.
      const onMediaSettled = (e: Event) => {
        if (e.target instanceof HTMLImageElement) {
          convergePasses = 0;
          schedule();
        }
      };
      view.dom.addEventListener("load", onMediaSettled, true);
      view.dom.addEventListener("error", onMediaSettled, true);

      // F7.13 — FONTES QUE CARREGAM DEPOIS DA MEDIÇÃO (FOUT). O KaTeX renderiza
      // a fórmula SÍNCRONO no mount (vide MathFormula), mas suas fontes (woff2)
      // chegam async; quando chegam, as métricas do glifo mudam e a altura do
      // bloco de fórmula ajusta levemente → poderia deixar um vão. `fonts.ready`
      // resolve UMA vez quando todas as fontes assentam — recalculamos então.
      // Cobre também qualquer outra web font do corpo. Discreto, não loopa.
      let fontsAlive = true;
      if (typeof document !== "undefined" && document.fonts?.ready) {
        void document.fonts.ready.then(() => {
          if (!fontsAlive) return;
          convergePasses = 0;
          schedule();
        });
      }

      // F7.14 — FONTES TARDIAS (KaTeX vem no chunk lazy de fórmulas). O
      // `fonts.ready` acima resolve UMA vez; se as fontes do KaTeX chegam DEPOIS
      // (carregadas junto do bundle de math, após o 1º ready), a fórmula foi
      // medida com fonte fallback (altura errada/gigante) e o vão NUNCA era
      // corrigido — só sumia com um Enter manual (que força recompute via doc
      // change). `loadingdone` dispara a CADA lote de fontes que assenta → re-
      // paginamos. Discreto, não loopa (spacer não baixa fonte). Os timers são
      // rede de segurança pro reflow do KaTeX após as fontes aplicarem.
      const onFontsLoadingDone = () => {
        if (!fontsAlive) return;
        convergePasses = 0;
        schedule();
      };
      try {
        document.fonts?.addEventListener?.("loadingdone", onFontsLoadingDone);
      } catch {
        /* navegador sem FontFaceSet events — ok */
      }
      const settleTimers = [250, 800, 1600].map((ms) =>
        window.setTimeout(() => {
          if (!fontsAlive) return;
          convergePasses = 0;
          schedule();
        }, ms),
      );

      // F7.15 — FÓRMULA (RE)RENDERIZOU. O nó de fórmula (MathFormula) é atom +
      // `ignoreMutation`, e o KaTeX vem em chunk lazy + fontes async: quando a
      // fórmula finalmente renderiza/reflui, a ALTURA do bloco muda mas NADA no
      // fluxo do ProseMirror avisa a paginação (não é mudança de doc). Era
      // exatamente por isso que a página só "consertava" com um Enter manual. O
      // MathFormula agora emite `sicro:math-rendered` ao (re)renderizar (com
      // guarda anti-laço) — re-paginamos no sinal. Discreto, debounced pelo rAF.
      const onMathRendered = () => {
        if (!fontsAlive) return;
        convergePasses = 0;
        schedule();
      };
      window.addEventListener("sicro:math-rendered", onMathRendered);

      // Cálculo inicial após mount.
      schedule();

      return {
        // F7.11 — Update do PM dispara após CADA transaction (incluindo
        // os setMeta do próprio plugin). Para quebrar o loop:
        //   - Se o DOC mudou (typing/paste/undo/setOpts re-render):
        //     schedule recompute.
        //   - Se SÓ decorations mudaram (nosso próprio dispatch):
        //     NÃO schedule. O estado de paginação já está atualizado.
        // Para `setOpts` (mudança de margem) o apply marca a transação,
        // mas como o doc NÃO mudou, precisamos detectar de outra forma.
        // Solução: comparamos opts em prevState vs current. Se opts
        // mudou, schedule.
        update: (view, prevState) => {
          const docChanged = !view.state.doc.eq(prevState.doc);
          const prevOpts = PAGINATION_PLUGIN_KEY.getState(prevState)?.opts;
          const currOpts = PAGINATION_PLUGIN_KEY.getState(view.state)?.opts;
          const optsChanged =
            prevOpts &&
            currOpts &&
            (prevOpts.marginTopCm !== currOpts.marginTopCm ||
              prevOpts.marginBottomCm !== currOpts.marginBottomCm ||
              prevOpts.marginLeftCm !== currOpts.marginLeftCm ||
              prevOpts.marginRightCm !== currOpts.marginRightCm ||
              prevOpts.gapCm !== currOpts.gapCm ||
              prevOpts.pageHeightCm !== currOpts.pageHeightCm ||
              prevOpts.mode !== currOpts.mode);
          if (docChanged || optsChanged) {
            // F7.17 — RESET da convergência a CADA mudança de doc/opts. O
            // `convergePasses` (limite M15 de re-agendamentos do modo split) é
            // de MÓDULO e vazava entre ciclos: inserir uma fórmula consome os
            // passes (bloco atômico + KaTeX async + hidratação setNodeMarkup);
            // a edição/remoção SEGUINTE entrava aqui com convergePasses já no
            // teto (3) → `convergePasses < MAX` falhava de cara → o loop de
            // convergência NUNCA mais rodava → a paginação TRAVAVA no estado
            // errado (margem quebrada que PERSISTIA mesmo após remover a
            // fórmula; só um load de imagem/fonte "destravava"). Toda mudança
            // de doc merece orçamento de convergência novo.
            convergePasses = 0;
            schedule();
          }
        },
        destroy: () => {
          view.dom.removeEventListener("load", onMediaSettled, true);
          view.dom.removeEventListener("error", onMediaSettled, true);
          try {
            document.fonts?.removeEventListener?.(
              "loadingdone",
              onFontsLoadingDone,
            );
          } catch {
            /* noop */
          }
          window.removeEventListener("sicro:math-rendered", onMathRendered);
          settleTimers.forEach((t) => window.clearTimeout(t));
          fontsAlive = false;
          if (raf != null) window.cancelAnimationFrame(raf);
        },
      };
    },
  });
}

/**
 * Atualiza opções do plugin em runtime e força recompute. Chame quando
 * o usuário alterar margens, gap, orientação, etc.
 */
export function setPaginationOptions(
  view: EditorView,
  opts: Partial<PaginationOptions>,
): void {
  view.dispatch(
    view.state.tr.setMeta(PAGINATION_PLUGIN_KEY, {
      type: "setOpts",
      opts,
    } satisfies Meta),
  );
}

/**
 * Mede o DOM do editor e produz a lista de spacers necessários para
 * paginar o conteúdo conforme `opts`.
 *
 * F7.6 — Todas as margens (top, bottom de cada página) são representadas
 * por SPACERS uniformes. O `editorWrap` tem padding-top/bottom ZERO; a
 * margem top da pg1 vem de um "spacer inicial" inserido em pos=0. Margens
 * entre páginas vêm dos spacers de page-break (gap + mBot + mTop). Margem
 * bottom da última página vem de um "spacer final" no fim do doc.
 *
 * Isso garante que cada página é uma ENTIDADE idêntica visualmente — não
 * importa se é pg1, pg2 ou pg10, todas têm o mesmo tratamento de margens.
 */
export function computePaginationDecos(
  view: EditorView,
  opts: PaginationOptions,
): DecorationSet {
  // Dispatcher por modo. "block" (estável, padrão) empurra o bloco inteiro pra
  // próxima página; "split" (Frente X, experimental) quebra o parágrafo no meio
  // (estilo Word). Isolados: o flag liga/desliga sem tocar o caminho estável.
  return opts.mode === "split"
    ? computeSplitDecos(view, opts)
    : computeBlockDecos(view, opts);
}

/**
 * Frente X (EXPERIMENTAL) — paginação que quebra o parágrafo no meio da página.
 * Cada bloco vira SEGMENTOS: parágrafo (<p>) = N linhas (corta no meio); bloco
 * atômico (figura/tabela/título…) = 1 segmento (empurra inteiro). Mede tudo no
 * layout natural (zera os spacers da rodada anterior numa passada). Reaproveita
 * a contagem por spacer (cards/multipage de graça). NÃO toca o caminho estável.
 */
function computeSplitDecos(
  view: EditorView,
  opts: PaginationOptions,
): DecorationSet {
  const marginTopPx = opts.marginTopCm * PX_PER_CM;
  const marginBottomPx = opts.marginBottomCm * PX_PER_CM;
  const usableHeightPx =
    (opts.pageHeightCm - opts.marginTopCm - opts.marginBottomCm) * PX_PER_CM;
  const minMiddleSpacerPx =
    (opts.gapCm + opts.marginBottomCm + opts.marginTopCm) * PX_PER_CM;
  if (usableHeightPx <= 0) return DecorationSet.empty;

  const SAFETY_PX = 6;
  const pageHeightPx = opts.pageHeightCm * PX_PER_CM;
  const gapPx = opts.gapCm * PX_PER_CM;
  const docSize = view.state.doc.content.size;
  const decorations: Decoration[] = [];

  // Spacer inicial — margem topo da pg1 (idêntico ao modo block).
  decorations.push(
    Decoration.widget(0, () => makeSpacer(marginTopPx, "top"), {
      side: -1,
      key: `margin-top-${Math.round(marginTopPx)}`,
      ignoreSelection: true,
    }),
  );

  const normalBlocks = (Array.from(view.dom.children) as HTMLElement[]).filter(
    (c) => !c.classList?.contains("sicro-page-spacer"),
  );

  // SEGMENTO = unidade mínima de quebra. Bloco ATÔMICO (figura/tabela/título/
  // etc.) = 1 segmento → empurra inteiro (igual ao block). PARÁGRAFO = N
  // segmentos (1 por LINHA visual) → permite cortar no meio da página. O
  // pmPos do PRIMEIRO segmento de cada bloco é a posição ENTRE-blocos (quebra
  // limpa antes do bloco); dos demais (linhas 2+) é a posição INLINE da linha
  // (quebra no meio do parágrafo).
  // Resolução PREGUIÇOSA do pmPos: a posição PM da linha só é calculada quando
  // uma quebra realmente cai nela (poucas vezes por doc) e via coordsAtPos
  // (off-screen-safe), nunca upfront pra cada linha.
  interface Seg {
    /** Posição PM já conhecida (entre-blocos / linha 0 / bloco atômico).
     *  `null` = resolver pelo `topAbs` na hora da quebra. */
    pmPos: number | null;
    /** Topo viewport-coord da linha (pra resolver o pmPos sob demanda). */
    topAbs: number;
    height: number;
  }

  // X5.3 — MEDIÇÃO NUMA PASSADA, NO LAYOUT NATURAL. Remove (display:none) TODOS
  // os spacers de paginação da rodada anterior de uma vez (top/middle/bottom +
  // inline), mede tudo (offsetTop, linhas, posAtCoords) sem o "empurrão" deles,
  // e restaura no finally. Síncrono → sem flicker. Robusto e rápido (1 reflow
  // pra remover + 1 pra restaurar). É o que mata o "buraco enorme" ao editar.
  // Usamos `display:none` (não `height:0`) porque o spacer do MEIO é
  // `inline-block` (vide makeSpacer) e height:0 deixaria o strut da linha como
  // resíduo; display:none tira tudo do fluxo.
  const allSpacers = Array.from(
    view.dom.querySelectorAll<HTMLElement>(".sicro-page-spacer"),
  );
  const savedDisplays = allSpacers.map((s) => s.style.display);
  for (const s of allSpacers) s.style.display = "none";

  let yOnCurrentPage = 0;
  let pageIndex = 0;

  try {
    for (let i = 0; i < normalBlocks.length; i++) {
      const block = normalBlocks[i]!;
      const next = normalBlocks[i + 1];

      // Spacers zerados → altura efetiva = offsetTop-diff natural (sem desconto).
      // max() com offsetHeight: garante que NUNCA contamos um bloco MENOR que a
      // própria altura renderizada. Importante p/ blocos atômicos (fórmula/figura)
      // cuja margem poderia colapsar pra fora do offsetTop-diff medido (com os
      // spacers ocultos) e voltar a aparecer no render (com o spacer presente),
      // fazendo o conteúdo VAZAR a margem inferior.
      const effectiveHeight = next
        ? Math.max(
            0,
            Math.floor(next.offsetTop - block.offsetTop),
            Math.floor(block.offsetHeight),
          )
        : Math.floor(block.offsetHeight);

      let posBeforeBlock: number;
      try {
        posBeforeBlock = Math.max(0, view.posAtDOM(block, 0) - 1);
      } catch {
        // Bloco sem posição PM resolvível: conta a altura e segue (não quebra).
        yOnCurrentPage += effectiveHeight;
        continue;
      }

      // Só <p> (parágrafo) é divisível por linha; o resto (títulos, figuras,
      // tabelas, formas, quesitos…) = 1 segmento (empurra inteiro).
      const splittable = block.tagName.toLowerCase() === "p";
      // Range PM do conteúdo do bloco (pra resolver o pmPos de uma linha via
      // busca binária com coordsAtPos, sob demanda, na hora da quebra).
      const node = view.state.doc.nodeAt(posBeforeBlock);
      const blockStartPmPos = posBeforeBlock + 1;
      const blockEndPmPos = node
        ? posBeforeBlock + node.nodeSize - 1
        : blockStartPmPos;

      let segments: Seg[];
      if (splittable) {
        const lines = measureLineBoxes(block);
        // Defesa anti-"buraco": uma linha NUNCA é mais alta que a página
        // inteira no layout natural. Se vier (glitch de medição transitório
        // após edição/margem), NÃO confiamos no split — degradamos pra empurrar
        // o bloco inteiro (= modo block), evitando spacer gigante no meio.
        const glitched = lines.some((ln) => ln.height > usableHeightPx);
        segments = glitched
          ? [{ pmPos: posBeforeBlock, topAbs: 0, height: effectiveHeight }]
          : lines.map((ln, idx) => ({
              // Linha 0 quebra ENTRE blocos (pmPos conhecido); linhas 2+ quebram
              // no MEIO (pmPos resolvido sob demanda pelo topAbs).
              pmPos: idx === 0 ? posBeforeBlock : null,
              topAbs: ln.topAbs,
              height: ln.height,
            }));
      } else {
        segments = [
          { pmPos: posBeforeBlock, topAbs: 0, height: effectiveHeight },
        ];
      }

      // Margem colapsada entre este bloco e o próximo: nem as line boxes nem o
      // offsetHeight a capturam, mas o effectiveHeight (offsetTop-diff) sim. Ela
      // é contada no preenchimento da página DEPOIS das linhas (sem quebrar a
      // última linha do parágrafo por causa da própria margem) — espelhando o
      // modo block (estável). Sem isto, "uma letra por linha" subconta ~1 margem
      // por <p>; o erro acumula e a margem superior das páginas seguintes
      // degrada (~1 linha por página).
      const measuredSum = segments.reduce((s, sg) => s + sg.height, 0);
      const trailingMarginPx = Math.max(0, effectiveHeight - measuredSum);

      for (const seg of segments) {
        if (
          yOnCurrentPage + seg.height > usableHeightPx - SAFETY_PX &&
          yOnCurrentPage > 0
        ) {
          // Resolve o pmPos da linha SÓ AGORA (quebra real), off-screen-safe.
          const breakPos =
            seg.pmPos !== null
              ? seg.pmPos
              : posAtLineStart(
                  view,
                  blockStartPmPos,
                  blockEndPmPos,
                  seg.topAbs,
                );
          const dynamicSpacerPx = Math.max(
            minMiddleSpacerPx,
            pageHeightPx + gapPx - yOnCurrentPage,
          );
          decorations.push(
            Decoration.widget(
              breakPos,
              () => makeSpacer(dynamicSpacerPx, "middle"),
              {
                side: -1,
                key: `pagebreak-split-${pageIndex + 1}-${breakPos}-${Math.round(dynamicSpacerPx)}`,
                ignoreSelection: true,
              },
            ),
          );
          pageIndex += 1;
          yOnCurrentPage = 0;
        }
        yOnCurrentPage += seg.height;
      }
      // Conta a margem colapsada até o próximo bloco (vide acima) no
      // preenchimento da página — sem quebrar a última linha deste parágrafo.
      yOnCurrentPage += trailingMarginPx;
    }
  } finally {
    // Restaura o display dos spacers (a medição já terminou).
    for (let i = 0; i < allSpacers.length; i++) {
      allSpacers[i]!.style.display = savedDisplays[i] ?? "";
    }
  }

  // Spacer final — margem bottom da última página.
  decorations.push(
    Decoration.widget(docSize, () => makeSpacer(marginBottomPx, "bottom"), {
      side: 1,
      key: `margin-bottom-${Math.round(marginBottomPx)}`,
      ignoreSelection: true,
    }),
  );

  return DecorationSet.create(view.state.doc, decorations);
}

/** Motor ESTÁVEL (padrão): empurra o bloco inteiro quando não cabe. */
function computeBlockDecos(
  view: EditorView,
  opts: PaginationOptions,
): DecorationSet {
  const marginTopPx = opts.marginTopCm * PX_PER_CM;
  const marginBottomPx = opts.marginBottomCm * PX_PER_CM;
  const usableHeightPx =
    (opts.pageHeightCm - opts.marginTopCm - opts.marginBottomCm) * PX_PER_CM;
  const middleSpacerPx =
    (opts.gapCm + opts.marginBottomCm + opts.marginTopCm) * PX_PER_CM;

  if (usableHeightPx <= 0) {
    return DecorationSet.empty;
  }

  const docSize = view.state.doc.content.size;
  const decorations: Decoration[] = [];

  // F7.6 — Spacer inicial: margem top da pg1.
  decorations.push(
    Decoration.widget(0, () => makeSpacer(marginTopPx, "top"), {
      side: -1,
      key: `margin-top-${Math.round(marginTopPx)}`,
      ignoreSelection: true,
    }),
  );

  // M3 — Algoritmo determinístico (sem `offsetTop`).
  //
  // Versões anteriores subtraíam `accumulatedSpacerPx` de `offsetTop` para
  // reconstruir uma "posição natural" e comparar com `usableHeightPx`. O
  // problema: a cada keystroke o browser desloca TODOS os blocos abaixo do
  // cursor em ±1px (sub-pixel rendering em telas hi-DPI). Blocos perto do
  // limiar trocavam de página de forma alternada — exatamente o sintoma
  // "tecla sim, tecla não, margem some" reportado pelo usuário.
  //
  // Substituí por simulação: percorro os blocos NORMAIS (ignorando os
  // spacers que NÓS mesmos inserimos), acumulo só `offsetHeight` em uma
  // variável `yOnCurrentPage`, e quebro a página quando o próximo bloco
  // não couber. `offsetHeight` muda em saltos discretos (uma linha
  // inteira quando o texto quebra), não em sub-pixels — então a decisão
  // é estável entre keystrokes.
  //
  // M4 — MIDDLE SPACER DINÂMICO.
  //
  // O middle spacer tem que empurrar o conteúdo "natural" (sem spacers)
  // até a posição visual desejada (topo da próxima página + marginTop).
  // Como blocos têm altura discreta, o último bloco de uma página deixa
  // FOLGA até a margem inferior (ex: termina em 22cm de 24.2cm úteis).
  // O constante `gap+mBot+mTop` só vale se a página estiver 100% cheia;
  // com folga, ele fica curto e a margem-topo da próxima página é
  // visualmente "comida" pela folga.
  //
  // Fórmula correta:
  //
  //     middleSpacer = (pageHeight + gap) - yOnCurrentPage
  //
  // Derivação: o primeiro bloco da próxima página deve ficar em
  // `pageStart_next + marginTop = (pageHeight + gap) + marginTop`.
  // Sem spacer, esse bloco estaria em `marginTop + yOnCurrentPage`.
  // Logo o spacer = (pageHeight + gap) + mTop - (mTop + yOn) = (ph+gap) - yOn.
  //
  // Quando yOn = usable (bloco preenche perfeito), o resultado = gap+mBot+mTop
  // (a constante antiga). Quando yOn < usable (folga), o spacer cresce
  // proporcionalmente — exatamente o que falta para alinhar a margem.
  //
  // Spacers existentes na DOM (do round anterior) são apenas IGNORADOS
  // aqui: não preciso "voltar atrás" a posição de nada porque não uso
  // offsetTop.
  // M6 — ALTURA EFETIVA = offsetTop_next - offsetTop_current.
  //
  // `offsetHeight` retorna só o content box do bloco, SEM as margens CSS.
  // Como cada `<p>` tem `margin: 1em` (~16px), o gap real entre dois
  // parágrafos no fluxo é maior que `offsetHeight`. Acumular só
  // offsetHeight subconta ~16px por bloco. Com 27 parágrafos isso vira
  // ~430px (11cm) de underflow acumulado — exatamente o que fazia o
  // algoritmo "achar" que cabia mais conteúdo na pg1 e deixar o texto
  // vazar para dentro da margem inferior.
  //
  // Fix: medir altura efetiva pelo deslocamento entre blocos vizinhos.
  // `next.offsetTop - block.offsetTop` capta:
  //   - o content box do bloco atual
  //   - + a margem colapsada entre ele e o próximo
  // Para o último bloco (sem next) cai pra offsetHeight (subconta a
  // margem final mas isso só afeta o spacer bottom da última pg).
  //
  // Sub-pixel stability: offsetTop é inteiro (px). A DIFERENÇA entre
  // dois irmãos depende só do conteúdo do bloco atual — quando blocos
  // antes dele mudam de altura, AMBOS offsetTops deslocam pelo mesmo
  // delta, e a diferença permanece estável. Mantém a determinismo do M4.
  //
  // Spacers existentes na DOM entre dois normalBlocks (nossos próprios
  // widgets de paginação) são descontados de offsetTop_next pra evitar
  // que o spacer infle a altura efetiva do bloco anterior.
  const SAFETY_PX = 6;
  const pageHeightPx = opts.pageHeightCm * PX_PER_CM;
  const gapPx = opts.gapCm * PX_PER_CM;
  const minMiddleSpacerPx = middleSpacerPx; // floor pra blocos absurdamente grandes
  const allChildren = Array.from(view.dom.children) as HTMLElement[];
  const normalBlocks = allChildren.filter(
    (c) => !c.classList?.contains("sicro-page-spacer"),
  );

  let yOnCurrentPage = 0;
  let pageIndex = 0;

  for (let i = 0; i < normalBlocks.length; i++) {
    const block = normalBlocks[i]!;
    const next = normalBlocks[i + 1];

    let effectiveHeight: number;
    if (next) {
      // Calcula altura efetiva via diferença de offsetTop, descontando
      // qualquer spacer (decoração nossa) que esteja entre os dois.
      let spacersBetweenPx = 0;
      let cursor = block.nextElementSibling as HTMLElement | null;
      while (cursor && cursor !== next) {
        if (cursor.classList?.contains("sicro-page-spacer")) {
          spacersBetweenPx += cursor.offsetHeight;
        }
        cursor = cursor.nextElementSibling as HTMLElement | null;
      }
      // max() com a própria altura do bloco: o diff de offsetTop captura as
      // margens colapsadas (M6), mas pode SUBCONTAR (medir ~0) em blocos atom
      // como fórmula/figura em layout transiente. offsetHeight nunca subconta a
      // altura real renderizada → evita o yOnCurrentPage subcontado que gerava
      // spacer ≈ página inteira (página em branco com fórmula).
      effectiveHeight = Math.max(
        0,
        Math.floor(next.offsetTop - block.offsetTop - spacersBetweenPx),
        Math.floor(block.offsetHeight),
      );
    } else {
      // Último bloco: sem next, use offsetHeight. Subconta a margem final,
      // mas isso só afeta a folga visual da margem bottom da última pg.
      effectiveHeight = Math.floor(block.offsetHeight);
    }

    // Quebra para a próxima página quando o bloco não cabe + safety.
    // `yOnCurrentPage > 0` evita loop infinito: um bloco maior que a
    // página inteira (ex: imagem gigante) NÃO dispara quebra se já
    // está no topo de uma página vazia — ele simplesmente vaza
    // visualmente (limitação conhecida, blocos não são quebrados).
    if (
      yOnCurrentPage + effectiveHeight > usableHeightPx - SAFETY_PX &&
      yOnCurrentPage > 0
    ) {
      let posBeforeChild: number | null = null;
      try {
        const inside = view.posAtDOM(block, 0);
        posBeforeChild = Math.max(0, inside - 1);
      } catch {
        // Bloco sem posição PM resolvível: pula sem quebrar.
        yOnCurrentPage += effectiveHeight;
        continue;
      }
      // Spacer DINÂMICO: cobre exatamente a folga da pg atual + área
      // entre páginas + marginTop da próxima pg.
      const dynamicSpacerPx = Math.max(
        minMiddleSpacerPx,
        pageHeightPx + gapPx - yOnCurrentPage,
      );
      decorations.push(
        Decoration.widget(
          posBeforeChild,
          () => makeSpacer(dynamicSpacerPx, "middle"),
          {
            side: -1,
            // Key inclui altura — força re-render quando dynamic muda.
            key: `pagebreak-${pageIndex + 1}-${Math.round(dynamicSpacerPx)}`,
            ignoreSelection: true,
          },
        ),
      );
      pageIndex += 1;
      yOnCurrentPage = 0;
    }

    yOnCurrentPage += effectiveHeight;
  }

  // F7.6 — Spacer final: margem bottom da última página.
  decorations.push(
    Decoration.widget(
      docSize,
      () => makeSpacer(marginBottomPx, "bottom"),
      {
        side: 1,
        key: `margin-bottom-${Math.round(marginBottomPx)}`,
        ignoreSelection: true,
      },
    ),
  );

  return DecorationSet.create(view.state.doc, decorations);
}

function makeSpacer(
  heightPx: number,
  variant: "top" | "middle" | "bottom",
): HTMLElement {
  const div = document.createElement("div");
  div.className = `sicro-page-spacer sicro-page-spacer--${variant}`;
  // F7.5/7.6 — Altura em CM para alinhar com os page cards (que estão em
  // cm). Browser arredonda CM identicamente em ambos os elementos.
  const heightCm = heightPx / PX_PER_CM;
  div.style.height = `${heightCm}cm`;
  div.style.userSelect = "none";
  div.style.pointerEvents = "none";
  div.contentEditable = "false";
  div.setAttribute("aria-hidden", "true");
  // Frente X — fidelidade Word: o spacer do MEIO (quebra de página) é
  // `inline-block` ocupando 100% da largura. Ele vai pra própria linha
  // (empurrando o resto pra próxima página), MAS a linha de texto ANTERIOR
  // deixa de ser "última linha" do bloco → o `justify` a ESTICA até a margem
  // (como no Word), enquanto a última linha REAL do parágrafo continua solta.
  // `vertical-align:top` + `line-height:0` garantem altura renderizada ==
  // heightCm (sem strut da linha) → sem drift no alinhamento dos cards.
  // (top/bottom seguem block: ficam nas bordas do doc, sem afetar justify.)
  if (variant === "middle") {
    div.style.display = "inline-block";
    div.style.width = "100%";
    div.style.verticalAlign = "top";
    div.style.lineHeight = "0";
  }
  return div;
}

function decorationSetsEqual(a: DecorationSet, b: DecorationSet): boolean {
  const aFound = a.find();
  const bFound = b.find();
  if (aFound.length !== bFound.length) return false;
  for (let i = 0; i < aFound.length; i++) {
    if (aFound[i]!.from !== bFound[i]!.from) return false;
    // Compara também o `key` para detectar mudança de spacer height
    // (key inclui o heightPx — vide makeSpacer).
    const aKey = (aFound[i] as unknown as { spec?: { key?: string } }).spec
      ?.key;
    const bKey = (bFound[i] as unknown as { spec?: { key?: string } }).spec
      ?.key;
    if (aKey !== bKey) return false;
  }
  return true;
}

/**
 * Conta quantos spacers (page breaks) estão atualmente ativos.
 */
export function countPaginationBreaks(view: EditorView | null): number {
  if (!view) return 0;
  const state = PAGINATION_PLUGIN_KEY.getState(view.state);
  if (!state) return 0;
  return state.decos.find().length;
}
