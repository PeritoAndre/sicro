/**
 * SicroTableView — NodeView da tabela no EDITOR (estende o TableView do
 * prosemirror-tables/TipTap).
 *
 * Por que estender em vez de um addNodeView próprio: o `@tiptap/extension-
 * table` injeta o NodeView através da opção `View` do plugin
 * `columnResizing`, que instancia `new View(node, defaultCellMinWidth, view)`.
 * Subclassando o `TableView` herdamos TODO o gerenciamento de `<colgroup>`
 * (larguras + resize ao vivo) de graça e só ADICIONAMOS:
 *
 *   - F4 — uma LEGENDA editável inline ("Tabela N — …") acima/abaixo da
 *     tabela. O número "Tabela N — " continua sendo decoração viva do
 *     AutoNumbering (renderizada à esquerda da legenda no editor); aqui
 *     editamos só o texto livre, persistido no attr `caption` da tabela.
 *
 *   - F4 — bordas/alinhamento/padding aplicados via classe/atributos `data-*`
 *     no `<table>` (o CSS lê esses atributos). Espelha o renderHTML pro
 *     clone estático/export.
 *
 * A legenda é um `contenteditable` que NÃO faz parte do documento
 * ProseMirror (o conteúdo da tabela é `tableRow+`). Comitamos o texto no
 * attr via transação no `blur`/`input` (debounce leve) e marcamos
 * `ignoreMutation` pra o PM não tentar reconciliar esse DOM.
 */

import { TableView, updateColumns } from "@tiptap/extension-table";
import type { Node as PMNode } from "@tiptap/pm/model";
import type { EditorView } from "@tiptap/pm/view";
import {
  DEFAULT_TABLE_BORDER_COLOR,
  DEFAULT_TABLE_BORDER_WIDTH,
  DEFAULT_TABLE_CELL_PADDING,
  type TableAlign,
  type TableBorderStyle,
} from "./SicroTable";

export class SicroTableView extends TableView {
  /** Wrapper externo que agrupa caption + tableWrapper como UM bloco
   *  (continua sendo um div, compatível com o tipo base HTMLDivElement). */
  private tableWrapper: HTMLElement;
  /** Linha da legenda: prefixo numerado (não-editável) + texto editável. */
  private captionPrefix: HTMLElement;
  private captionEl: HTMLElement;
  private captionRemoveBtn: HTMLElement;
  private view: EditorView | null;

  constructor(node: PMNode, cellMinWidth: number, view?: EditorView) {
    super(node, cellMinWidth);
    this.view = view ?? null;

    // `super` montou: this.dom = .tableWrapper > table(colgroup + tbody).
    // Re-embrulhamos: criamos um bloco externo que contém a legenda + o
    // tableWrapper original. Mantemos this.contentDOM/table/colgroup
    // apontando pros elementos originais (resize continua funcionando).
    const originalWrapper = this.dom;
    originalWrapper.classList.add("sicro-table-wrapper");
    this.tableWrapper = originalWrapper;

    const block = document.createElement("div");
    block.className = "sicro-table-block";
    // Inserimos o wrapper original dentro do bloco.
    block.appendChild(originalWrapper);

    // Linha da legenda (abaixo da tabela, como o figcaption do Figure):
    //   [Tabela N — ] prefixo numerado, NÃO-editável (espelha AutoNumbering)
    //   [texto livre] editável → attr `caption`
    //   [✕] botão de remover a legenda (hover) — estilo Word
    const row = document.createElement("div");
    row.className = "sicro-table-caption-row";
    row.setAttribute("data-table-caption", "true");
    // FRONTEIRA DE EDIÇÃO — o fix do "clico e nada acontece". A linha inteira
    // é `contenteditable=false`; só o span do texto volta a ser `true`. Sem a
    // fronteira false no meio, o Chromium NÃO move o foco pra um
    // contenteditable aninhado dentro do contenteditable do ProseMirror (o
    // atributo vira no-op) — o caret ficava no editor raiz, o PM relia a
    // seleção e a legenda nunca recebia foco. Com a fronteira, o span vira um
    // editing host PRÓPRIO: clique foca, digitação flui, e o PM ignora o
    // subtree não-editável (stopEvent + ignoreMutation cobrem o resto).
    row.contentEditable = "false";

    const prefix = document.createElement("span");
    prefix.className = "sicro-auto-number-table";
    prefix.setAttribute("data-auto-number", "true");
    row.appendChild(prefix);
    this.captionPrefix = prefix;

    const caption = document.createElement("span");
    caption.className = "sicro-table-caption sicro-table-caption--editable";
    caption.setAttribute("data-table-caption-edit", "true");
    // plaintext-only (Chromium/WebView2): colar conteúdo rico vira texto puro
    // — sem <b>/<span style> residuais no DOM da legenda (o attr só guarda
    // textContent). O listener de paste abaixo é o cinto extra (multi-linha).
    caption.contentEditable = "plaintext-only";
    caption.setAttribute("role", "textbox");
    caption.setAttribute("aria-label", "Legenda da tabela");
    caption.spellcheck = true;
    row.appendChild(caption);
    this.captionEl = caption;

    const removeBtn = document.createElement("button");
    removeBtn.type = "button";
    removeBtn.className = "sicro-table-caption-remove";
    removeBtn.title = "Remover legenda (pode recriar pelo botão direito)";
    removeBtn.setAttribute("aria-label", "Remover legenda da tabela");
    removeBtn.textContent = "✕";
    row.appendChild(removeBtn);
    this.captionRemoveBtn = removeBtn;

    block.appendChild(row);

    // O NodeView passa a expor o bloco externo como `dom`.
    this.dom = block;

    this.syncCaption();
    this.applyPresentation();
    this.wireCaptionEvents();
  }

  /** Tabela de LAYOUT (bloco de registro/timbre): sem bordas. Não recebe
   *  número nem legenda automática — espelha a regra do numberFigures. */
  private isLayoutTable(): boolean {
    return (this.node.attrs.borderStyle as string | null) === "none";
  }

  /** Região do editor (corpo/cabeçalho/rodapé). Tabelas em cabeçalho/rodapé
   *  são institucionais/layout (ex.: bloco de registro) e NÃO recebem a
   *  legenda numerada "Tabela N — …" — senão ela "pisca" (aparece no NodeView
   *  ao editar, some no clone estático ao desfocar) e atrapalha o cabeçalho.
   *  Detectado pelo `data-sicro-region` que useHeaderEditor/useFooterEditor
   *  põem no DOM do editor. */
  private isHeaderFooterRegion(): boolean {
    const dom = this.view?.dom as HTMLElement | undefined;
    const region = dom?.getAttribute?.("data-sicro-region");
    return region === "header" || region === "footer";
  }

  /** Tabela NUMERÁVEL = exibição de conteúdo com legenda visível. Tabelas de
   *  layout (sem borda) e tabelas com legenda REMOVIDA (captionVisible=false)
   *  não consomem número — espelhado em numbering.ts, AutoNumbering e
   *  extractTables (manter os 4 em sincronia). */
  private static isNumerable(attrs: Record<string, unknown>): boolean {
    return (
      (attrs.borderStyle as string | null) !== "none" &&
      attrs.captionVisible !== false
    );
  }

  /** Ordinal da tabela (1-based) por ordem de aparição no documento, contando
   *  só tabelas NUMERÁVEIS (vide isNumerable). Independe de `id` — robusto pra
   *  docs legados. */
  private computeOrdinal(): number {
    if (!this.view) return 1;
    let selfPos: number | null = null;
    try {
      selfPos = this.view.posAtDOM(this.tableWrapper, 0);
    } catch {
      selfPos = null;
    }
    // Resolve a posição do PRÓPRIO nó table (before).
    let selfTablePos = -1;
    if (selfPos != null && selfPos >= 0) {
      const $p = this.view.state.doc.resolve(
        Math.min(selfPos, this.view.state.doc.content.size),
      );
      for (let d = $p.depth; d >= 0; d--) {
        if ($p.node(d).type.name === "table") {
          selfTablePos = $p.before(d);
          break;
        }
      }
    }
    let count = 0;
    let ordinal = 1;
    let found = false;
    this.view.state.doc.descendants((n, pos) => {
      if (n.type.name === "table") {
        if (SicroTableView.isNumerable(n.attrs as Record<string, unknown>)) {
          count += 1;
          if (pos === selfTablePos) {
            ordinal = count;
            found = true;
          }
        }
        return false; // não desce em linhas/células (sem tabelas aninhadas)
      }
      return true;
    });
    return found ? ordinal : count || 1;
  }

  /** Reflete o texto do attr `caption` no DOM da legenda + o prefixo
   *  "Tabela N — " numerado (sem mexer no cursor se já estiver focado).
   *  Tabelas de LAYOUT (sem borda) escondem a linha de legenda inteira. */
  private syncCaption() {
    const captionRow = this.captionEl.parentElement;
    if (
      this.isLayoutTable() ||
      this.isHeaderFooterRegion() ||
      this.node.attrs.captionVisible === false
    ) {
      if (captionRow) captionRow.style.display = "none";
      return;
    }
    if (captionRow) captionRow.style.display = "";

    // Modo Leitura: legenda visível mas não-editável (os handlers também têm
    // gate em view.editable; aqui é o reflexo visual — cursor/✕).
    const editable = this.view?.editable ?? true;
    this.captionEl.contentEditable = editable ? "plaintext-only" : "false";
    this.captionRemoveBtn.style.display = editable ? "" : "none";

    const text = String(this.node.attrs.caption ?? "");
    if (document.activeElement === this.captionEl) {
      // Não sobrescreve enquanto o usuário digita (evita reset de cursor).
      if (this.captionEl.textContent !== text) {
        // só atualiza se realmente divergiu (ex.: undo externo)
        this.captionEl.textContent = text;
      }
    } else if (this.captionEl.textContent !== text) {
      this.captionEl.textContent = text;
    }
    // Prefixo numerado (decoração viva — espelha o AutoNumbering antigo).
    const ordinal = this.computeOrdinal();
    this.captionPrefix.textContent = `Tabela ${ordinal} — `;
    // Placeholder visual quando vazio.
    this.captionEl.classList.toggle(
      "sicro-table-caption--empty",
      text.trim() === "",
    );
  }

  /** Aplica bordas/align/padding no `<table>` via atributos `data-*` +
   *  estilo de margem. O CSS faz o resto (mesmas regras do clone estático). */
  private applyPresentation() {
    const a = this.node.attrs;
    const align = (a.tableAlign as TableAlign | null) ?? "left";
    const border = (a.borderStyle as TableBorderStyle | null) ?? "all";
    const color = (a.borderColor as string | null) ?? DEFAULT_TABLE_BORDER_COLOR;
    const width = Number(a.borderWidth ?? DEFAULT_TABLE_BORDER_WIDTH);
    const pad = Number(a.cellPadding ?? DEFAULT_TABLE_CELL_PADDING);

    this.table.setAttribute("data-sicro-table", "true");
    this.table.setAttribute("data-table-align", align);
    this.table.setAttribute("data-border-style", border);
    this.table.setAttribute("data-border-color", color);
    this.table.setAttribute("data-border-width", String(width));
    this.table.setAttribute("data-cell-padding", String(pad));
    if (a.id) this.table.setAttribute("data-table-id", String(a.id));

    // Variáveis CSS pra bordas/padding (lidas pelas regras em styles.css),
    // assim cor/espessura/padding customizados valem sem inline em cada td.
    this.table.style.setProperty("--sicro-table-border-color", color);
    this.table.style.setProperty("--sicro-table-border-width", `${width}px`);
    this.table.style.setProperty("--sicro-table-cell-padding", `${pad}px`);

    // Alinhamento da tabela na página.
    if (align === "center") {
      this.tableWrapper.style.marginLeft = "auto";
      this.tableWrapper.style.marginRight = "auto";
    } else if (align === "right") {
      this.tableWrapper.style.marginLeft = "auto";
      this.tableWrapper.style.marginRight = "0";
    } else {
      this.tableWrapper.style.marginLeft = "0";
      this.tableWrapper.style.marginRight = "auto";
    }
  }

  private commitTimer: ReturnType<typeof setTimeout> | null = null;

  /** Posição PM do PRÓPRIO nó table no doc (resolvida pelo DOM). `null` se o
   *  NodeView ainda não está conectado/resolvível. */
  private resolveTablePos(): number | null {
    if (!this.view) return null;
    let pos: number | null = null;
    try {
      pos = this.view.posAtDOM(this.tableWrapper, 0);
    } catch {
      pos = null;
    }
    if (pos == null || pos < 0) return null;
    // posAtDOM(tableWrapper, 0) cai DENTRO da tabela; a posição do nó table
    // é a do bloco que a contém. Resolvemos subindo até o nó `table`.
    const { state } = this.view;
    const $pos = state.doc.resolve(Math.min(pos, state.doc.content.size));
    for (let d = $pos.depth; d >= 0; d--) {
      if ($pos.node(d).type.name === "table") return $pos.before(d);
    }
    return null;
  }

  /** Aplica um patch de attrs no nó table (transação undoable). */
  private patchTableAttrs(patch: Record<string, unknown>) {
    if (!this.view) return;
    const tablePos = this.resolveTablePos();
    if (tablePos == null) return;
    const { state } = this.view;
    const node = state.doc.nodeAt(tablePos);
    if (!node || node.type.name !== "table") return;
    const tr = state.tr.setNodeMarkup(tablePos, undefined, {
      ...node.attrs,
      ...patch,
    });
    tr.setMeta("addToHistory", true);
    this.view.dispatch(tr);
  }

  private commitCaption() {
    // Somente-leitura (modo Leitura): nunca modifica o doc por aqui.
    if (!this.view?.editable) return;
    const text = this.captionEl.textContent ?? "";
    if (text === String(this.node.attrs.caption ?? "")) return;
    this.patchTableAttrs({ caption: text });
  }

  /** Remove a legenda (estilo Word): apaga o texto e esconde a linha. A tabela
   *  deixa de consumir um número "Tabela N". Recriável pelo botão direito
   *  ("Adicionar legenda") — vide tableOps/TableOverlay. */
  private removeCaption() {
    if (!this.view?.editable) return;
    if (this.commitTimer) {
      clearTimeout(this.commitTimer);
      this.commitTimer = null;
    }
    this.captionEl.blur();
    this.patchTableAttrs({ caption: "", captionVisible: false });
    // Devolve o foco ao editor: Ctrl+Z desfaz a remoção de imediato (sem o
    // focus, o atalho não chega ao ProseMirror até o próximo clique).
    this.view?.focus();
  }

  /** Foca o texto da legenda com o caret no fim (clique no prefixo/na linha). */
  private focusCaptionEnd() {
    this.captionEl.focus();
    try {
      const range = document.createRange();
      range.selectNodeContents(this.captionEl);
      range.collapse(false);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    } catch {
      /* caret best-effort */
    }
  }

  private wireCaptionEvents() {
    // Commit IMEDIATO a cada input (sem debounce): elimina a janela de perda
    // de texto (fechar o app / trocar de laudo com a legenda focada matava o
    // timer pendente sem comitar — destroy() não pode dispatchar). O undo não
    // vira 1 passo por tecla: o prosemirror-history agrupa setNodeMarkup
    // adjacentes pelo newGroupDelay (500ms). O syncCaption pós-commit roda
    // com activeElement === span e textContent já igual ao attr → não reseta
    // o cursor (verificado).
    this.captionEl.addEventListener("input", () => {
      if (!this.view?.editable) return;
      this.captionEl.classList.toggle(
        "sicro-table-caption--empty",
        (this.captionEl.textContent ?? "").trim() === "",
      );
      this.commitCaption();
    });
    this.captionEl.addEventListener("blur", () => {
      this.commitCaption();
    });
    // Paste em TEXTO PURO (cinto extra ao plaintext-only): normaliza
    // multi-linha pra espaço único — legenda é uma linha só.
    this.captionEl.addEventListener("paste", (e) => {
      e.preventDefault();
      if (!this.view?.editable) return;
      const text = (e.clipboardData?.getData("text/plain") ?? "")
        .replace(/\s+/g, " ")
        .trim();
      // execCommand preserva o undo nativo do contenteditable no Chromium.
      document.execCommand("insertText", false, text);
    });
    // Enter na legenda não deve quebrar linha — confirma e sai. Backspace com
    // a legenda VAZIA remove a linha inteira (estilo Word). Esc sai sem mais.
    this.captionEl.addEventListener("keydown", (e) => {
      if (!this.view?.editable) {
        e.preventDefault();
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        this.captionEl.blur();
      } else if (
        e.key === "Backspace" &&
        (this.captionEl.textContent ?? "").trim() === ""
      ) {
        e.preventDefault();
        this.removeCaption();
      } else if (e.key === "Escape") {
        e.preventDefault();
        this.captionEl.blur();
      }
    });
    // Botão ✕ — remove a legenda. mousedown+preventDefault evita roubar o
    // foco antes do click.
    this.captionRemoveBtn.addEventListener("mousedown", (e) => {
      e.preventDefault();
    });
    this.captionRemoveBtn.addEventListener("click", (e) => {
      e.preventDefault();
      this.removeCaption();
    });
    // Clique no prefixo "Tabela N — " ou na área vazia da linha → foca o
    // texto editável (a linha é contenteditable=false; sem isto o clique
    // fora do span não faria nada).
    const row = this.captionEl.parentElement;
    row?.addEventListener("mousedown", (e) => {
      if (!this.view?.editable) return;
      const target = e.target as globalThis.Node;
      if (
        target === this.captionEl ||
        this.captionEl.contains(target) ||
        target === this.captionRemoveBtn ||
        this.captionRemoveBtn.contains(target)
      ) {
        return; // o próprio span/botão tratam
      }
      e.preventDefault();
      this.focusCaptionEnd();
    });
  }

  update(node: PMNode): boolean {
    if (node.type !== this.node.type) return false;
    this.node = node;
    // Re-sincroniza colunas (resize) — mesma lógica do TableView base.
    updateColumns(node, this.colgroup, this.table, this.cellMinWidth);
    this.syncCaption();
    this.applyPresentation();
    return true;
  }

  ignoreMutation(mutation: MutationRecord | { type: string; target: globalThis.Node }): boolean {
    const target = mutation.target as globalThis.Node;
    // Mutations na legenda (texto/childList) ou no prefixo numerado são
    // nossas — PM deve ignorar (não fazem parte do doc ProseMirror).
    const captionRow = this.captionEl.parentElement;
    if (
      this.captionEl.contains(target) ||
      target === this.captionEl ||
      this.captionPrefix.contains(target) ||
      target === this.captionPrefix ||
      (captionRow && (target === captionRow || captionRow.contains(target)))
    ) {
      return true;
    }
    // Resto: delega à lógica do TableView base (colgroup, etc.).
    // O TableView base trata mutations fora do contentDOM mas dentro do
    // wrapper como ignoráveis; replicamos pra o tableWrapper original.
    const isInsideContent = this.contentDOM.contains(target);
    const isInsideWrapper = this.tableWrapper.contains(target);
    if (isInsideWrapper && !isInsideContent) {
      if (
        mutation.type === "attributes" ||
        mutation.type === "childList" ||
        mutation.type === "characterData"
      ) {
        return true;
      }
    }
    // Mutations no bloco externo (ex: nossos data-* no <table>) → ignora.
    if (this.dom.contains(target) && !isInsideContent) {
      if (mutation.type === "attributes") return true;
    }
    return false;
  }

  /**
   * Eventos (mousedown/clique/teclas/input) na LINHA DA LEGENDA são tratados
   * pelo BROWSER, não pelo ProseMirror. A legenda é um `contenteditable`
   * manual que vive FORA do `contentDOM` do PM — sem este `stopEvent`, o PM
   * intercepta o clique, devolve o foco/seleção pro corpo do editor e a
   * legenda fica IMPOSSÍVEL de editar (era exatamente o sintoma: clicar e
   * nada acontecer). Retornando `true` aqui, o PM ignora o evento e deixa o
   * contenteditable funcionar (foco, digitação, Backspace pra apagar tudo).
   */
  stopEvent(event: Event): boolean {
    const target = event.target as globalThis.Node | null;
    if (!target) return false;
    const captionRow = this.captionEl.parentElement;
    return (
      this.captionEl.contains(target) ||
      target === this.captionEl ||
      (!!captionRow && (target === captionRow || captionRow.contains(target)))
    );
  }

  destroy() {
    if (this.commitTimer) clearTimeout(this.commitTimer);
  }
}
