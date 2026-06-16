/**
 * EditorToolbar — formatting controls + insert actions for the laudo editor.
 *
 * Spike B exposes the minimal set: heading levels, inline marks, lists,
 * alignment, and four insert actions (figure, table, storyboard, system data).
 * Save and HTML preview live here too because users expect them near the toolbar.
 */

import type { Editor } from "@tiptap/react";
import {
  AlignCenter,
  AlignJustify,
  AlignLeft,
  IndentDecrease,
  IndentIncrease,
  AlignRight,
  ArrowRight,
  Bold,
  Circle,
  Code,
  Eraser,
  Eye,
  Highlighter,
  Italic,
  KeyRound,
  Landmark,
  List,
  ListOrdered,
  Minus,
  Save,
  Search,
  Shapes,
  Sigma,
  Square,
  Strikethrough,
  Subscript as SubscriptIcon,
  Superscript as SuperscriptIcon,
  Table as TableIcon,
  TextCursorInput,
  Underline as UnderlineIcon,
} from "lucide-react";
// (useState já é importado mais abaixo no arquivo via o sub-componente
//  ColorPickerBtn; reaproveitamos aqui também.)
import { NodeSelection } from "@tiptap/pm/state";
import { MathEditDialog, type MathDialogContext } from "./MathEditDialog";
import { EDIT_MATH_EVENT } from "../document-engine/nodes";
import { commands } from "@core/commands";
import { useSigdocsStore } from "@stores/sigdocsStore";
import { SigdocsCredentialsDialog } from "./SigdocsCredentialsDialog";
import { exportLaudo } from "../services/laudoExport";
import { ExportMenu } from "./ExportMenu";
import type { SicroDoc } from "../document-engine";
// F1.2 — Bloco de registro com colwidths semeados (larga/média/estreita).
import { generateTableId, registrationBlockColWidths } from "../document-engine";
import styles from "./EditorToolbar.module.css";

// F2 — Edição rica.
//
// Catálogo de fontes. Inclui todas as fontes nativas do Windows
// (deployment alvo) + fallbacks razoáveis pra macOS/Linux. Browser
// faz fallback gracioso se a fonte específica não estiver instalada.
// Caller pode usar `setFontFamily(family)` com qualquer string CSS
// válida se precisar de algo fora desta lista.
const FONT_FAMILIES: ReadonlyArray<{ value: string; label: string }> = [
  // "" só aparece em seleção MISTA (fontes diferentes no trecho). Sem fonte
  // explícita o combo mostra a fonte EFETIVA (Arial, o padrão do documento) —
  // nunca um genérico "fonte padrão".
  { value: "", label: "—" },
  // Sans-serif comuns
  { value: "Arial, sans-serif", label: "Arial" },
  { value: "'Arial Black', sans-serif", label: "Arial Black" },
  { value: "'Arial Narrow', sans-serif", label: "Arial Narrow" },
  { value: "Bahnschrift, sans-serif", label: "Bahnschrift" },
  { value: "Calibri, sans-serif", label: "Calibri" },
  { value: "'Calibri Light', sans-serif", label: "Calibri Light" },
  { value: "Candara, sans-serif", label: "Candara" },
  { value: "'Century Gothic', sans-serif", label: "Century Gothic" },
  { value: "Corbel, sans-serif", label: "Corbel" },
  { value: "'Franklin Gothic Medium', sans-serif", label: "Franklin Gothic Medium" },
  { value: "Gadugi, sans-serif", label: "Gadugi" },
  { value: "Helvetica, Arial, sans-serif", label: "Helvetica" },
  { value: "Impact, sans-serif", label: "Impact" },
  { value: "'Lucida Sans Unicode', sans-serif", label: "Lucida Sans Unicode" },
  { value: "'Microsoft Sans Serif', sans-serif", label: "Microsoft Sans Serif" },
  { value: "'MS Sans Serif', sans-serif", label: "MS Sans Serif" },
  { value: "'Segoe UI', sans-serif", label: "Segoe UI" },
  { value: "'Segoe UI Light', sans-serif", label: "Segoe UI Light" },
  { value: "'Segoe UI Semibold', sans-serif", label: "Segoe UI Semibold" },
  { value: "'Segoe UI Black', sans-serif", label: "Segoe UI Black" },
  { value: "Tahoma, sans-serif", label: "Tahoma" },
  { value: "'Trebuchet MS', sans-serif", label: "Trebuchet MS" },
  { value: "Verdana, sans-serif", label: "Verdana" },
  { value: "'Yu Gothic UI', sans-serif", label: "Yu Gothic UI" },
  // Serif (texto institucional)
  { value: "Cambria, serif", label: "Cambria" },
  { value: "'Cambria Math', serif", label: "Cambria Math" },
  { value: "Constantia, serif", label: "Constantia" },
  { value: "Garamond, serif", label: "Garamond" },
  { value: "Georgia, serif", label: "Georgia" },
  { value: "'Palatino Linotype', serif", label: "Palatino Linotype" },
  { value: "'Book Antiqua', serif", label: "Book Antiqua" },
  { value: "'Bookman Old Style', serif", label: "Bookman Old Style" },
  { value: "Rockwell, serif", label: "Rockwell" },
  { value: "Sylfaen, serif", label: "Sylfaen" },
  { value: "'Times New Roman', serif", label: "Times New Roman" },
  { value: "'MS Serif', serif", label: "MS Serif" },
  // Monospace
  { value: "Consolas, monospace", label: "Consolas" },
  { value: "'Courier New', monospace", label: "Courier New" },
  { value: "'Lucida Console', monospace", label: "Lucida Console" },
  // Display / decorativas (uso pontual em capa, marca d'água)
  { value: "'Comic Sans MS', cursive", label: "Comic Sans MS" },
  { value: "'Brush Script MT', cursive", label: "Brush Script MT" },
  { value: "'Ink Free', cursive", label: "Ink Free" },
  { value: "'MV Boli', cursive", label: "MV Boli" },
  { value: "Gabriola, serif", label: "Gabriola" },
];

// Tamanhos padrão do dropdown (estilo Word, até 72). O combobox é
// DIGITÁVEL — o usuário pode informar qualquer número fora desta lista
// (inteiros ou meios, ex.: 10.5, 53). Estes são só os atalhos prontos.
const FONT_SIZES_PT: ReadonlyArray<number> = [
  6, 7, 8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72,
];

// Faixa aceita ao digitar um tamanho à mão.
const FONT_SIZE_MIN = 1;
const FONT_SIZE_MAX = 200;

/**
 * Converte uma string CSS de tamanho (ex.: "12pt", "16px") para o número
 * em pt exibido no campo. Aceita só valores em pt (o resto do app sempre
 * grava `${n}pt`); para qualquer outra unidade devolve `null` para não
 * exibir um número enganoso no input.
 */
function parseFontSizePt(value: string | undefined | null): number | null {
  if (!value) return null;
  const m = /^([0-9]+(?:\.[0-9]+)?)\s*pt$/i.exec(value.trim());
  if (!m) return null;
  const n = Number.parseFloat(m[1]!);
  return Number.isFinite(n) ? n : null;
}

/** Formata um número de pt para exibição no campo (sem zeros à toa). */
function formatPt(n: number): string {
  return String(n);
}

/**
 * Lê um atributo da marca `textStyle` ao longo da seleção ATUAL.
 *  - Cursor recolhido → o valor no ponto do cursor (incl. marca "stored").
 *  - Seleção em faixa → o valor único quando TODO o texto selecionado o
 *    compartilha; `MIXED` quando há valores diferentes (o controle deve
 *    ficar vazio, como no Word).
 *
 * Retorna `undefined` quando não há valor (== padrão), uma string com o
 * valor, ou o sentinela `MIXED`.
 */
const MIXED = Symbol("mixed");
function readTextStyleAttr(
  editor: Editor,
  attr: "fontSize" | "fontFamily" | "color",
): string | undefined | typeof MIXED {
  const { state } = editor;
  const { from, to, empty } = state.selection;
  if (empty) {
    const a = editor.getAttributes("textStyle") as Record<string, unknown>;
    const v = a[attr];
    return typeof v === "string" && v ? v : undefined;
  }
  // Faixa: percorre os nós de texto e coleta o valor da marca textStyle.
  let seen: string | undefined;
  let initialized = false;
  let mixed = false;
  state.doc.nodesBetween(from, to, (node) => {
    if (mixed) return false;
    if (!node.isText) return undefined;
    const mark = node.marks.find((m) => m.type.name === "textStyle");
    const raw = mark?.attrs?.[attr];
    const v = typeof raw === "string" && raw ? raw : undefined;
    if (!initialized) {
      seen = v;
      initialized = true;
    } else if (seen !== v) {
      mixed = true;
    }
    return undefined;
  });
  if (mixed) return MIXED;
  return seen;
}

// As paletas FONT_COLORS / HIGHLIGHT_COLORS antigas foram substituídas pelo
// `ColorPickerPro` (S/V pad + hue slider + paleta de 40 + recentes), que
// vive em `ColorPickerPro.tsx`. O wrapper `ColorPickerBtn` no fim deste
// arquivo apenas abre o picker como popover.

interface EditorToolbarProps {
  editor: Editor | null;
  isSaving: boolean;
  isPreviewOpen: boolean;
  onSave: () => void;
  onTogglePreview: () => void;
  /**
   * F2 — abre a barra de localizar/substituir. Quando omitido, o botão
   * "Localizar" fica oculto.
   */
  onOpenFind?: () => void;
  /** Workspace + laudo id + current doc; required only by the export menu.
   *  When `laudoId`/`workspacePath` are missing the export menu is hidden.
   *  `laudoTitle` surfaces which laudo will be exported, so the user can't
   *  accidentally export the wrong one. */
  workspacePath?: string;
  laudoId?: string;
  laudoTitle?: string;
  doc?: SicroDoc | null;
  occurrence?: Record<string, unknown> | null;
}

export function EditorToolbar({
  editor,
  isSaving,
  isPreviewOpen,
  onSave,
  onTogglePreview,
  onOpenFind,
  workspacePath,
  laudoId,
  laudoTitle,
  doc,
  occurrence,
}: EditorToolbarProps) {
  // J — Estado do cover do SIGDOC. Click no botão SIGDOCS abre o
  // portal cobrindo o editor. Fluxo completo:
  //   1. Pega URL do SIGDOC do manifest.
  //   2. Mede bounds do .body do editor (via querySelector — viável
  //      porque sabemos a estrutura do LaudoEditorView).
  //   3. Abre o webview cover. ResizeObserver no host mantém posição.
  // Se houver um laudo finalizado com PDF disponível, também exporta
  // o PDF e abre o Explorer (fluxo "assinar" comum). Senão, só abre.
  const sigdocsCoverOpen = useSigdocsStore((s) => s.coverOpen);
  const setSigdocsCoverOpen = useSigdocsStore((s) => s.setCoverOpen);
  // K — Modal de gerenciamento de credenciais SIGDOC.
  const [credentialsDialogOpen, setCredentialsDialogOpen] = useState(false);

  // Pós-laudo — Reatividade por SELEÇÃO. A toolbar lê `editor.getAttributes`
  // e `editor.isActive` no render, mas o React não re-renderiza quando só a
  // SELEÇÃO muda (clicar num texto sem digitar). Sem isto, escrever em 12pt
  // e clicar num trecho de 20pt mantinha o controle mostrando 12. Assinamos
  // os eventos do editor ativo e forçamos um re-render para que TODOS os
  // indicadores (tamanho, família, cor, isActive de bold/itálico/…) reflitam
  // o ponto do cursor ao vivo. Re-assina quando o `editor` troca (corpo →
  // cabeçalho → rodapé).
  const [, forceUpdate] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    if (!editor) return undefined;
    const bump = () => forceUpdate();
    editor.on("selectionUpdate", bump);
    editor.on("transaction", bump);
    // Sincroniza uma vez ao montar / trocar de editor (a região recém-ativa
    // pode já ter um cursor posicionado).
    bump();
    return () => {
      editor.off("selectionUpdate", bump);
      editor.off("transaction", bump);
    };
  }, [editor]);

  const handleToggleSigdocs = async () => {
    if (!workspacePath) return;
    if (sigdocsCoverOpen) {
      setSigdocsCoverOpen(false);
      await commands.closeSigdocsCover().catch(() => {
        /* silent */
      });
      return;
    }
    try {
      // K — Antes de abrir o SIGDOC, exporta o PDF do laudo atual e
      // abre o Explorer na pasta (revealAfter=true por padrão no
      // helper). Assim o perito já tem o arquivo pronto pra arrastar
      // pro portal (que bloqueia Ctrl+V).
      if (laudoId && doc) {
        try {
          const { pushToast, dismissToast } = await import(
            "@/components/toast/toastStore"
          );
          const toastId = pushToast("progress", "Gerando PDF do laudo…", {
            title: "Preparando para SIGDOC",
          });
          try {
            await exportLaudo(
              "pdf",
              workspacePath,
              laudoId,
              doc,
              (occurrence as Record<string, unknown> | null) ?? null,
            );
            dismissToast(toastId);
            pushToast(
              "success",
              "PDF pronto na pasta — arraste para o SIGDOC após o login.",
              { title: "PDF exportado", durationMs: 6000 },
            );
          } catch (err) {
            dismissToast(toastId);
            pushToast(
              "warn",
              `PDF não foi gerado (${(err as Error)?.message ?? "erro"}). Você ainda pode usar o SIGDOC normalmente.`,
            );
          }
        } catch {
          /* toast load failed — non-fatal */
        }
      }

      // Abre o cover SIGDOC.
      const cfg = await commands.getSigdocsUrl(workspacePath);
      const bodyEl = document.querySelector<HTMLElement>(
        '[data-sigdocs-cover-body="1"]',
      );
      const rect = bodyEl?.getBoundingClientRect();
      const headerH = 36; // mesmo do SigdocsCoverHost
      const initial = rect
        ? {
            x: rect.left,
            y: rect.top + headerH,
            width: rect.width,
            height: Math.max(50, rect.height - headerH),
          }
        : { x: 200, y: 120, width: 1000, height: 600 };
      await commands.openSigdocsCover(cfg.url, initial);
      setSigdocsCoverOpen(true);
    } catch (err) {
      console.warn("[SIGDOC] cover open failed", err);
    }
  };
  if (!editor) {
    return <div className={styles.toolbar} aria-hidden />;
  }

  const currentHeading: string = editor.isActive("heading", { level: 1 })
    ? "h1"
    : editor.isActive("heading", { level: 2 })
      ? "h2"
      : editor.isActive("heading", { level: 3 })
        ? "h3"
        : "p";

  const setStructure = (value: string) => {
    if (value === "p") {
      editor.chain().focus().setParagraph().run();
    } else {
      const level = Number(value.slice(1)) as 1 | 2 | 3;
      editor.chain().focus().toggleHeading({ level }).run();
    }
  };

  // F2 — Helpers para cor, realce, fonte, tamanho.
  // Atributos lidos ao longo da SELEÇÃO atual (ver `readTextStyleAttr`), de
  // modo que os controles reflitam o ponto do cursor ao vivo (a reatividade
  // por seleção é garantida pelo useEffect acima). Seleção com valores
  // mistos → controle vazio (estilo Word).
  const rawFamily = readTextStyleAttr(editor, "fontFamily");
  const rawSize = readTextStyleAttr(editor, "fontSize");
  const rawColor = readTextStyleAttr(editor, "color");

  // Defaults Arial 12pt (padrão do documento — vide --font-doc). Quando o
  // usuário ainda não escolheu fonte/tamanho explicitamente (e não há
  // mistura), mostramos a fonte EFETIVA no combo — nunca "fonte padrão".
  // Em seleção mista a família mostra "—" e o tamanho fica vazio.
  const fontFamilyMixed = rawFamily === MIXED;
  const currentFontFamily = fontFamilyMixed
    ? ""
    : (rawFamily ?? "Arial, sans-serif");

  // Tamanho como NÚMERO em pt (exibido sem "pt" no campo). `null` = vazio
  // (seleção mista ou unidade não-pt).
  const fontSizeMixed = rawSize === MIXED;
  const currentFontSizePt = fontSizeMixed
    ? null
    : (parseFontSizePt(typeof rawSize === "string" ? rawSize : null) ?? 12);

  const colorMixed = rawColor === MIXED;
  const currentColor = colorMixed
    ? ""
    : (typeof rawColor === "string" ? rawColor : "");

  // Pós-laudo S — atributos de espaçamento do bloco ativo (parágrafo
  // ou heading mais próximo). Usados pelos controles de line-height +
  // space before/after da toolbar.
  const blockAttrs = (editor.getAttributes("paragraph") ?? {}) as {
    line_height?: number;
    space_before_pt?: number;
    space_after_pt?: number;
    first_line_indent_cm?: number;
  };
  const headingAttrs = (editor.getAttributes("heading") ?? {}) as {
    line_height?: number;
    space_before_pt?: number;
    space_after_pt?: number;
    first_line_indent_cm?: number;
  };
  const currentLineHeight =
    Number(blockAttrs.line_height) || Number(headingAttrs.line_height) || 0;
  const currentSpaceBefore =
    Number(blockAttrs.space_before_pt) ||
    Number(headingAttrs.space_before_pt) ||
    0;
  const currentSpaceAfter =
    Number(blockAttrs.space_after_pt) ||
    Number(headingAttrs.space_after_pt) ||
    0;

  const setLineHeight = (raw: string) => {
    const v = Number.parseFloat(raw);
    editor
      .chain()
      .focus()
      .setParagraphLineHeight(Number.isFinite(v) ? v : 0)
      .run();
  };
  const setSpaceBefore = (raw: string) => {
    const v = Number.parseFloat(raw);
    editor
      .chain()
      .focus()
      .setParagraphSpaceBefore(Number.isFinite(v) ? v : 0)
      .run();
  };
  const setSpaceAfter = (raw: string) => {
    const v = Number.parseFloat(raw);
    editor
      .chain()
      .focus()
      .setParagraphSpaceAfter(Number.isFinite(v) ? v : 0)
      .run();
  };

  // "Compactar ¶" — conserta laudos importados de .docx que vinham com uma LINHA
  // EM BRANCO (Enter duplo) entre cada parágrafo, o que espalhava o conteúdo e
  // dava aparência de margem não respeitada. Remove os parágrafos VAZIOS de topo
  // e aplica um espaço-depois REAL (estilo Word, 6pt) nos demais — separação por
  // espaçamento de parágrafo em vez de linhas vazias. Reversível com Ctrl+Z.
  const COMPACT_SPACE_AFTER_PT = 6;
  const compactParagraphs = () => {
    const { state } = editor;
    const tr = state.tr;
    const empties: { from: number; to: number }[] = [];
    let kept = 0;
    state.doc.descendants((node, pos, parent) => {
      if (node.type.name !== "paragraph") return undefined;
      const topLevel = parent?.type.name === "doc";
      if (node.content.size === 0 && topLevel) {
        // Parágrafo vazio de topo → marca pra remover (não toca em vazios
        // dentro de células de tabela, que a tabela exige).
        empties.push({ from: pos, to: pos + node.nodeSize });
      } else {
        kept += 1;
        // setNodeMarkup só muda attrs → NÃO altera tamanho → posições estáveis.
        tr.setNodeMarkup(pos, undefined, {
          ...node.attrs,
          space_after_pt: COMPACT_SPACE_AFTER_PT,
        });
      }
      return false; // não desce no parágrafo
    });
    // Remove os vazios do ÚLTIMO pro PRIMEIRO (posições continuam válidas porque
    // o setNodeMarkup acima não mexe em tamanhos). Só se sobrar conteúdo.
    if (kept > 0) {
      for (let i = empties.length - 1; i >= 0; i--) {
        tr.delete(empties[i]!.from, empties[i]!.to);
      }
    }
    if (tr.docChanged) {
      editor.view.dispatch(tr);
      editor.view.focus();
    }
  };

  const setFontFamily = (value: string) => {
    if (!value) {
      editor.chain().focus().unsetFontFamily().run();
    } else {
      editor.chain().focus().setFontFamily(value).run();
    }
  };
  const setFontSize = (value: string) => {
    if (!value) {
      editor.chain().focus().unsetFontSize().run();
    } else {
      editor.chain().focus().setFontSize(value).run();
    }
  };
  // Aplica um tamanho numérico em pt (clampado à faixa sensata). Reaplica
  // sempre como `${n}pt` — o resto do app espera essa unidade.
  const applyFontSizePt = (n: number) => {
    const clamped = Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, n));
    // Arredonda a no máx. 1 casa (aceita meios, ex.: 10.5).
    const rounded = Math.round(clamped * 10) / 10;
    setFontSize(`${rounded}pt`);
  };
  const setColor = (color: string) => {
    editor.chain().focus().setColor(color).run();
  };
  const unsetColor = () => {
    editor.chain().focus().unsetColor().run();
  };
  const setHighlight = (color: string) => {
    editor.chain().focus().toggleHighlight({ color }).run();
  };
  const unsetHighlight = () => {
    editor.chain().focus().unsetHighlight().run();
  };

  // Limpar formatação — remove todas as marcas e converte para parágrafo.
  const clearFormatting = () => {
    editor.chain().focus().unsetAllMarks().clearNodes().run();
  };

  return (
    <div className={styles.toolbar} role="toolbar" aria-label="Barra de ferramentas do laudo">
      <div className={styles.group}>
        <select
          className={styles.select}
          value={currentHeading}
          onChange={(e) => setStructure(e.target.value)}
          aria-label="Estilo do bloco"
          title="Estilo do bloco"
        >
          <option value="p">Texto</option>
          <option value="h1">Título</option>
          <option value="h2">Seção</option>
          <option value="h3">Subseção</option>
        </select>
        <select
          className={styles.select}
          value={currentFontFamily}
          onChange={(e) => setFontFamily(e.target.value)}
          aria-label="Fonte"
          title="Família da fonte"
          style={{ minWidth: 110 }}
        >
          {FONT_FAMILIES.map((f) => (
            <option key={f.value} value={f.value}>
              {f.label}
            </option>
          ))}
        </select>
        <FontSizeCombo
          valuePt={currentFontSizePt}
          onApply={applyFontSizePt}
        />
        {/* Cor do texto — ao LADO do tamanho (estilo Word). Swatch reflete a
            cor do cursor; `<input type="color">` nativo + botão limpar. O
            marca-texto continua no grupo de marcas, mais adiante. */}
        <TextColorSwatch
          current={currentColor}
          onSelect={setColor}
          onClear={unsetColor}
        />
      </div>

      {/* Pós-laudo S — Grupo de espaçamento (entrelinhas + antes/depois). */}
      <div className={styles.group} title="Espaçamento do parágrafo">
        <select
          className={styles.select}
          value={currentLineHeight || ""}
          onChange={(e) => setLineHeight(e.target.value)}
          aria-label="Entrelinhas"
          title="Espaçamento entre linhas"
          style={{ minWidth: 70 }}
        >
          <option value="">Linhas</option>
          <option value="1">1.0</option>
          <option value="1.15">1.15</option>
          <option value="1.5">1.5</option>
          <option value="2">2.0</option>
          <option value="2.5">2.5</option>
          <option value="3">3.0</option>
        </select>
        <label
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            fontSize: 11,
            color: "var(--sicro-fg-dim)",
          }}
          title="Espaço antes do parágrafo (em pontos)"
        >
          ↑
          <input
            type="number"
            min={0}
            max={72}
            step={1}
            value={currentSpaceBefore || ""}
            placeholder="0"
            onChange={(e) => setSpaceBefore(e.target.value)}
            style={{
              width: 48,
              padding: "2px 4px",
              fontSize: 11,
              background: "#fff",
              color: "#1a2a4a",
              border: "1px solid var(--sicro-border)",
              borderRadius: 3,
            }}
            aria-label="Espaço antes (pt)"
          />
          pt
        </label>
        <label
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            fontSize: 11,
            color: "var(--sicro-fg-dim)",
          }}
          title="Espaço depois do parágrafo (em pontos)"
        >
          ↓
          <input
            type="number"
            min={0}
            max={72}
            step={1}
            value={currentSpaceAfter || ""}
            placeholder="0"
            onChange={(e) => setSpaceAfter(e.target.value)}
            style={{
              width: 48,
              padding: "2px 4px",
              fontSize: 11,
              background: "#fff",
              color: "#1a2a4a",
              border: "1px solid var(--sicro-border)",
              borderRadius: 3,
            }}
            aria-label="Espaço depois (pt)"
          />
          pt
        </label>
        <button
          type="button"
          onClick={compactParagraphs}
          title="Compactar parágrafos: remove as linhas em branco (Enter duplo do .docx) e aplica espaçamento real de 6pt entre parágrafos, estilo Word. Reversível com Ctrl+Z."
          style={{
            fontSize: 11,
            padding: "3px 8px",
            marginLeft: 4,
            background: "#fff",
            color: "#1a2a4a",
            border: "1px solid var(--sicro-border)",
            borderRadius: 3,
            cursor: "pointer",
            whiteSpace: "nowrap",
          }}
        >
          Compactar ¶
        </button>
      </div>

      <div className={styles.group}>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive("bold")}
          onClick={() => editor.chain().focus().toggleBold().run()}
          label="Negrito (Ctrl+B)"
        >
          <Bold size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive("italic")}
          onClick={() => editor.chain().focus().toggleItalic().run()}
          label="Itálico (Ctrl+I)"
        >
          <Italic size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive("underline")}
          onClick={() => editor.chain().focus().toggleUnderline().run()}
          label="Sublinhado (Ctrl+U)"
        >
          <UnderlineIcon size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive("strike")}
          onClick={() => editor.chain().focus().toggleStrike().run()}
          label="Tachado (Ctrl+Shift+X)"
        >
          <Strikethrough size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive("subscript")}
          onClick={() => editor.chain().focus().toggleSubscript().run()}
          label="Subscrito"
        >
          <SubscriptIcon size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive("superscript")}
          onClick={() => editor.chain().focus().toggleSuperscript().run()}
          label="Sobrescrito"
        >
          <SuperscriptIcon size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive("code")}
          onClick={() => editor.chain().focus().toggleCode().run()}
          label="Código inline"
        >
          <Code size={14} />
        </ToolBtn>
      </div>

      <div className={styles.group}>
        {/* A cor do TEXTO foi movida pra junto do tamanho (acima). Aqui fica
            só o marca-texto (highlight), que continua com o picker rico. */}
        <ColorPickerBtn
          icon={<Highlighter size={14} />}
          label="Marca-texto"
          current={editor.getAttributes("highlight")["color"] as string | undefined}
          onSelect={setHighlight}
          onClear={unsetHighlight}
        />
        <ToolBtn
          editor={editor}
          isActive={false}
          onClick={clearFormatting}
          label="Limpar formatação"
        >
          <Eraser size={14} />
        </ToolBtn>
      </div>

      <div className={styles.group}>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive("bulletList")}
          onClick={() => editor.chain().focus().toggleBulletList().run()}
          label="Lista com marcadores"
        >
          <List size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive("orderedList")}
          onClick={() => editor.chain().focus().toggleOrderedList().run()}
          label="Lista numerada"
        >
          <ListOrdered size={14} />
        </ToolBtn>
      </div>

      <div className={styles.group}>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive({ textAlign: "left" })}
          onClick={() => editor.chain().focus().setTextAlign("left").run()}
          label="Alinhar à esquerda"
        >
          <AlignLeft size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive({ textAlign: "center" })}
          onClick={() => editor.chain().focus().setTextAlign("center").run()}
          label="Centralizar"
        >
          <AlignCenter size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive({ textAlign: "right" })}
          onClick={() => editor.chain().focus().setTextAlign("right").run()}
          label="Alinhar à direita"
        >
          <AlignRight size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={editor.isActive({ textAlign: "justify" })}
          onClick={() => editor.chain().focus().setTextAlign("justify").run()}
          label="Justificar"
        >
          <AlignJustify size={14} />
        </ToolBtn>
      </div>

      {/* R2 — Recuo à ESQUERDA do parágrafo (estilo Word). Aplica só no
           bloco onde o cursor está — diferente da margem da página (régua).
           Passo de 1,25cm; atalhos Ctrl+[ / Ctrl+]. */}
      <div className={styles.group}>
        <ToolBtn
          editor={editor}
          isActive={false}
          onClick={() => {
            const { $from } = editor.state.selection;
            let cur = 0;
            for (let d = $from.depth; d >= 0; d--) {
              const n = $from.node(d);
              if (n.type.name === "paragraph" || n.type.name === "heading") {
                cur = Number(n.attrs.left_indent_cm) || 0;
                break;
              }
            }
            editor.chain().focus().setLeftIndent(Math.max(0, cur - 1.25)).run();
          }}
          label="Diminuir recuo do parágrafo (Ctrl+[)"
        >
          <IndentDecrease size={14} />
        </ToolBtn>
        <ToolBtn
          editor={editor}
          isActive={false}
          onClick={() => {
            const { $from } = editor.state.selection;
            let cur = 0;
            for (let d = $from.depth; d >= 0; d--) {
              const n = $from.node(d);
              if (n.type.name === "paragraph" || n.type.name === "heading") {
                cur = Number(n.attrs.left_indent_cm) || 0;
                break;
              }
            }
            editor.chain().focus().setLeftIndent(Math.min(8, cur + 1.25)).run();
          }}
          label="Aumentar recuo do parágrafo (Ctrl+])"
        >
          <IndentIncrease size={14} />
        </ToolBtn>
      </div>

      {/* F7 — Removida a antiga seção "Inserir" da toolbar (Figura,
           Tabela, Storyboard, Quesito, Assinatura, Dado do sistema).
           Cada item agora vem por outro canal:
             - Figura/Prancha → menu superior "Figuras" (F6)
             - Tabela         → menu superior "Tabela" (F7, contextual)
             - Storyboard     → Inspector aba "Evidências"
             - Quesito        → estilo `quesito` no menu "Estilos" (Ctrl+Alt+6)
             - Assinatura     → templates já trazem; estilo `assinatura`
             - Dado do sistema → menu superior "Campos" (F5) com `{{var}}`
           A toolbar fica focada em FORMATAÇÃO de texto. */}

      {/* Q — Botão Inserir Forma com dropdown */}
      <InsertShapeMenu editor={editor} />

      {/* U — Botão Inserir Caixa de Texto. Insere TextBox flutuante com
          borda padrão + parágrafo vazio dentro. Controles de borda/fill/
          rotação aparecem no overlay quando a caixa é selecionada. */}
      <InsertTextBoxBtn editor={editor} />

      {/* Fórmula matemática (LaTeX) — editor visual MathLive. */}
      <InsertMathBtn editor={editor} />

      {/* V2 — Bloco de registro 1×3 (estilo cabeçalho de laudo): tabela com
          borda e 3 colunas (dados / nº do laudo / folha). Insere no editor
          ATIVO — funciona tanto no corpo quanto no cabeçalho (a toolbar
          mira o headerEditor quando editingRegion === "header"). */}
      <InsertRegistrationBlockBtn editor={editor} />

      <div className={styles.spacer} />

      {onOpenFind && (
        <button
          type="button"
          className={styles.btnLabel}
          onClick={onOpenFind}
          title="Localizar (Ctrl+F)"
        >
          <Search size={14} /> Localizar
        </button>
      )}

      <button
        type="button"
        className={`${styles.btnLabel} ${isPreviewOpen ? styles.active : ""}`}
        onClick={onTogglePreview}
        aria-pressed={isPreviewOpen}
      >
        <Eye size={14} /> Prévia HTML
      </button>

      {workspacePath && laudoId && (
        <ExportMenu
          workspacePath={workspacePath}
          laudoId={laudoId}
          laudoTitle={laudoTitle}
          doc={doc ?? null}
          occurrence={occurrence ?? null}
        />
      )}

      {/* J/K — Botão SIGDOC: exporta PDF + abre Explorer + cobre área
          do laudo com o portal. Credenciais salvas → autofill no login. */}
      {workspacePath && (
        <>
          <button
            type="button"
            className={`${styles.btnLabel} ${sigdocsCoverOpen ? styles.active : ""}`}
            onClick={() => void handleToggleSigdocs()}
            aria-pressed={sigdocsCoverOpen}
            title={
              sigdocsCoverOpen
                ? "Fechar painel SIGDOC"
                : "Exportar PDF + abrir SIGDOC para assinatura"
            }
          >
            <Landmark size={14} />{" "}
            {sigdocsCoverOpen ? "Fechar SIGDOC" : "SIGDOC"}
          </button>
          <button
            type="button"
            className={styles.btn}
            onClick={() => setCredentialsDialogOpen(true)}
            title="Gerenciar credenciais do SIGDOC (autofill)"
            aria-label="Gerenciar credenciais do SIGDOC"
          >
            <KeyRound size={13} />
          </button>
        </>
      )}

      <button
        type="button"
        className={styles.primary}
        onClick={onSave}
        disabled={isSaving}
        title="Salvar (Ctrl+S)"
      >
        <Save size={14} /> {isSaving ? "Salvando…" : "Salvar"}
      </button>

      {/* K — Modal de credenciais SIGDOC (autofill no cover). */}
      <SigdocsCredentialsDialog
        open={credentialsDialogOpen}
        onClose={() => setCredentialsDialogOpen(false)}
      />
    </div>
  );
}

interface ToolBtnProps {
  editor: Editor;
  isActive: boolean;
  onClick: () => void;
  label: string;
  children: React.ReactNode;
}

function ToolBtn({ isActive, onClick, label, children }: ToolBtnProps) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      className={`${styles.btn} ${isActive ? styles.active : ""}`}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

// ===========================================================================
// ColorPickerBtn — botão de cor que abre o ColorPickerPro como popover.
//
// O picker completo (S/V pad, hue slider, hex input, paleta de 40, recentes)
// vive em `ColorPickerPro.tsx`. Este componente só cuida do botão da
// toolbar + abertura/fechamento do popover com click-outside.

import { useEffect, useReducer, useRef, useState } from "react";
import { ColorPickerPro } from "./ColorPickerPro";

interface ColorPickerBtnProps {
  icon: React.ReactNode;
  label: string;
  current?: string | null;
  onSelect: (color: string) => void;
  onClear: () => void;
}

function ColorPickerBtn({
  icon,
  label,
  current,
  onSelect,
  onClear,
}: ColorPickerBtnProps) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    const handler = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [open]);

  return (
    <div className={styles.colorWrap} ref={wrapRef}>
      <button
        type="button"
        title={label}
        aria-label={label}
        className={styles.btn}
        onClick={() => setOpen((v) => !v)}
        style={{
          position: "relative",
          paddingBottom: 14,
        }}
      >
        {icon}
        <span
          className={styles.colorSwatch}
          style={{ background: current || "transparent" }}
          aria-hidden
        />
      </button>
      {open && (
        <div className={styles.colorMenu} role="menu">
          <ColorPickerPro
            current={current}
            onSelect={(hex) => {
              onSelect(hex);
              setOpen(false);
            }}
            onClear={() => {
              onClear();
              setOpen(false);
            }}
          />
        </div>
      )}
    </div>
  );
}

// ============================================================================
// FontSizeCombo — combobox DIGITÁVEL de tamanho de fonte (estilo Word).
//
// `<input>` digitável + botão de caret (▼) que abre um DROPDOWN CUSTOM
// (popover estilizado) com os tamanhos padrão (até 72). O usuário pode:
//   - DIGITAR qualquer número (inteiro ou meio, ex.: 10.5, 53), aplicado
//     no Enter ou no blur;
//   - ABRIR o caret e clicar num tamanho da lista (aplica na hora).
//
// Por que dropdown custom em vez de `<datalist>`: a `<datalist>` nativa é
// não-confiável no WebView2/Chromium do app — ela filtra as opções pelo
// valor atual do input e não mostra o caret nem abre de forma previsível.
// O popover custom (mesma mecânica de click-fora/Esc dos outros menus da
// toolbar) abre sempre e lista TODOS os tamanhos.
//
// O valor exibido segue o cursor (vem da prop `valuePt`); seleção mista →
// campo vazio. O campo guarda o texto enquanto é editado e só commita ao
// confirmar — assim dá pra digitar "53" sem reaplicar a cada tecla.
// ============================================================================

function FontSizeCombo({
  valuePt,
  onApply,
}: {
  valuePt: number | null;
  onApply: (n: number) => void;
}) {
  // `draft` = o que está no input. `null` significa "espelhar a prop".
  const [draft, setDraft] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const display = draft ?? (valuePt != null ? formatPt(valuePt) : "");

  const commit = (raw: string) => {
    const trimmed = raw.trim().replace(",", ".");
    setDraft(null); // volta a espelhar a prop após commitar
    if (!trimmed) return; // vazio → não mexe (mantém o atual)
    const n = Number.parseFloat(trimmed);
    if (!Number.isFinite(n)) return;
    onApply(n);
  };

  // Fecha em clique-fora (mesma mecânica dos outros popovers da toolbar:
  // ColorPickerBtn / InsertShapeMenu). O input em si vive dentro do wrap,
  // então digitar/focar não fecha o popover.
  useEffect(() => {
    if (!open) return undefined;
    const handler = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [open]);

  const pick = (n: number) => {
    setDraft(null);
    setOpen(false);
    onApply(n);
    // Devolve o foco ao input pra o fluxo de digitação continuar natural.
    requestAnimationFrame(() => inputRef.current?.focus());
  };

  return (
    <div className={styles.sizeCombo} ref={wrapRef}>
      <input
        ref={inputRef}
        type="text"
        inputMode="decimal"
        className={styles.sizeInput}
        value={display}
        aria-label="Tamanho da fonte (em pontos)"
        title="Tamanho da fonte — digite qualquer número (pt) ou escolha na lista"
        placeholder="—"
        autoComplete="off"
        role="combobox"
        aria-expanded={open}
        aria-controls="sicro-font-size-listbox"
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit((e.target as HTMLInputElement).value);
            setOpen(false);
            (e.target as HTMLInputElement).blur();
          } else if (e.key === "Escape") {
            setDraft(null);
            if (open) {
              // Esc fecha o popover primeiro (sem perder o foco do input).
              e.preventDefault();
              setOpen(false);
            } else {
              (e.target as HTMLInputElement).blur();
            }
          } else if (e.key === "ArrowDown" && !open) {
            // Seta pra baixo abre a lista (padrão de combobox).
            e.preventDefault();
            setOpen(true);
          }
        }}
        onBlur={(e) => {
          // Não commita se o blur foi pra dentro do popover (clique num
          // tamanho) — o `pick` cuida disso. Sem essa guarda, o blur
          // commitaria o texto digitado e o clique no item viria depois,
          // gerando duas aplicações.
          const next = e.relatedTarget as Node | null;
          if (next && wrapRef.current?.contains(next)) return;
          commit(e.target.value);
        }}
      />
      <button
        type="button"
        className={styles.sizeCaret}
        // mousedown em vez de click: dispara ANTES do blur do input, então
        // o toggle vê o estado `open` correto (um clique fecha o que abriu).
        onMouseDown={(e) => {
          e.preventDefault(); // não rouba o foco do input
          setOpen((v) => !v);
        }}
        aria-label="Mostrar tamanhos de fonte"
        title="Mostrar tamanhos de fonte"
        tabIndex={-1}
      >
        ▼
      </button>
      {open && (
        <div
          className={styles.sizeMenu}
          role="listbox"
          id="sicro-font-size-listbox"
          aria-label="Tamanhos de fonte"
        >
          {FONT_SIZES_PT.map((s) => {
            const isCurrent = valuePt != null && Math.abs(valuePt - s) < 0.001;
            return (
              <button
                key={s}
                type="button"
                role="option"
                aria-selected={isCurrent}
                className={`${styles.sizeMenuItem} ${isCurrent ? styles.sizeMenuItemActive : ""}`}
                // mousedown (não click) pra aplicar antes do blur do input.
                onMouseDown={(e) => {
                  e.preventDefault();
                  pick(s);
                }}
              >
                {formatPt(s)}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ============================================================================
// TextColorSwatch — seletor de cor do texto no MESMO visual do TextBox/Shape:
// um swatch quadrado mostrando a cor atual + `<input type="color">` nativo por
// cima. Botão "×" ao lado limpa a cor (unsetColor). O swatch reflete a cor do
// cursor (vazio/sem cor → quadriculado de "transparente").
// ============================================================================

function TextColorSwatch({
  current,
  onSelect,
  onClear,
}: {
  current: string;
  onSelect: (hex: string) => void;
  onClear: () => void;
}) {
  const hasColor = !!current;
  return (
    <span className={styles.textColorWrap}>
      <label
        className={styles.textColorSwatch}
        style={
          hasColor
            ? { background: current }
            : undefined /* sem cor → fundo quadriculado via CSS */
        }
        title={
          hasColor ? `Cor do texto (${current})` : "Cor do texto"
        }
      >
        <span className={styles.textColorGlyph} aria-hidden>
          A
        </span>
        <input
          type="color"
          value={normalizeColor(current)}
          aria-label="Cor do texto"
          onChange={(e) => onSelect(e.target.value)}
        />
      </label>
      <button
        type="button"
        className={styles.textColorClear}
        onClick={onClear}
        title="Remover cor do texto"
        aria-label="Remover cor do texto"
      >
        ×
      </button>
    </span>
  );
}

/** Normaliza cor pra `#RRGGBB` aceito por `<input type="color">`. */
function normalizeColor(color: string): string {
  if (!color) return "#000000";
  if (/^#[0-9a-f]{6}$/i.test(color)) return color;
  if (/^#[0-9a-f]{3}$/i.test(color)) {
    const r = color[1];
    const g = color[2];
    const b = color[3];
    return `#${r}${r}${g}${g}${b}${b}`;
  }
  return "#000000";
}

// ============================================================================
// Q — InsertShapeMenu: dropdown pra inserir formas (rect, ellipse, arrow, line)
// ============================================================================

function InsertShapeMenu({ editor }: { editor: Editor }) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    const onDocDown = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDocDown);
    return () => document.removeEventListener("mousedown", onDocDown);
  }, [open]);

  const insert = (kind: "rectangle" | "ellipse" | "arrow" | "line") => {
    // Centro aproximado da página de laudo (A4 com margens típicas).
    // Wrap_x/wrap_y são relativos ao .sicro-editor-content. Posiciona
    // um pouco offset pra não cair sempre no mesmo ponto se inserir várias.
    const jitter = Math.random() * 1.5;
    editor.commands.insertShape({
      kind,
      wrap_mode: "in_front",
      wrap_x_cm: 4 + jitter,
      wrap_y_cm: 4 + jitter,
    });
    setOpen(false);
  };

  return (
    <div ref={wrapRef} style={{ position: "relative" }}>
      <button
        type="button"
        className={styles.btnLabel}
        onClick={() => setOpen((v) => !v)}
        title="Inserir forma"
      >
        <Shapes size={14} /> Forma
      </button>
      {open && (
        <div
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            marginTop: 4,
            background: "#fff",
            border: "1px solid #c9d2e6",
            borderRadius: 6,
            boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
            padding: 4,
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: 2,
            minWidth: 180,
            zIndex: 100,
          }}
        >
          <button
            type="button"
            onClick={() => insert("rectangle")}
            style={shapeMenuItemStyle}
            title="Retângulo"
          >
            <Square size={14} />
            <span>Retângulo</span>
          </button>
          <button
            type="button"
            onClick={() => insert("ellipse")}
            style={shapeMenuItemStyle}
            title="Elipse / Círculo"
          >
            <Circle size={14} />
            <span>Elipse</span>
          </button>
          <button
            type="button"
            onClick={() => insert("arrow")}
            style={shapeMenuItemStyle}
            title="Seta"
          >
            <ArrowRight size={14} />
            <span>Seta</span>
          </button>
          <button
            type="button"
            onClick={() => insert("line")}
            style={shapeMenuItemStyle}
            title="Linha"
          >
            <Minus size={14} />
            <span>Linha</span>
          </button>
        </div>
      )}
    </div>
  );
}

const shapeMenuItemStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 6,
  padding: "6px 10px",
  border: "none",
  background: "transparent",
  cursor: "pointer",
  fontSize: 12,
  color: "#1a2a4a",
  borderRadius: 3,
  textAlign: "left",
};

/**
 * U — Botão "Caixa de texto". Insere um TextBox flutuante (in_front)
 * com border padrão preto-fino e parágrafo vazio dentro. Controles de
 * border/fill/rotação aparecem no overlay quando a caixa é selecionada.
 *
 * Pós-laudo U fix — Insere na posição do CURSOR (não mais hardcoded em
 * 4cm/4cm). Pega coords screen do caret via `coordsAtPos`, descobre
 * onde elas caem dentro do offsetParent que vai hospedar a textbox
 * (mesmo positioned ancestor que o navegador usa pro `position: absolute`)
 * e converte pra cm.
 *
 * Sem dropdown — single button, ação direta. O perito clica, a caixa
 * aparece próxima ao cursor, ele seleciona e ajusta visualmente.
 */
function InsertTextBoxBtn({ editor }: { editor: Editor }) {
  const handleClick = () => {
    const PX_PER_CM = 37.7952755906;
    let wrap_x_cm = 4;
    let wrap_y_cm = 4;

    try {
      const view = editor.view;
      const cursorPos = view.state.selection.from;
      const cursorCoords = view.coordsAtPos(cursorPos);
      // O dom do editor (`.ProseMirror`) é o ancestral mais próximo de
      // qualquer node inserido. Usamos ele como referência pra cm —
      // alinha bem com onde a textbox vai ser renderizada (offsetParent
      // do textbox geralmente é o `.ProseMirror` ou um ancestral próximo).
      const editorDom = view.dom as HTMLElement;
      const editorRect = editorDom.getBoundingClientRect();
      const zoom =
        editorRect.width / Math.max(1, editorDom.offsetWidth || 1);
      // Posição do cursor relativa ao topo-esq do editorDom, em layout px.
      const dxPx = (cursorCoords.left - editorRect.left) / Math.max(0.0001, zoom);
      const dyPx = (cursorCoords.top - editorRect.top) / Math.max(0.0001, zoom);
      // Pequeno offset pra caixa não ficar exatamente sob o cursor (que
      // some debaixo dela). Joga 1cm pra baixo do cursor.
      wrap_x_cm = Math.max(0, dxPx / PX_PER_CM);
      wrap_y_cm = Math.max(0, dyPx / PX_PER_CM + 0.3);
    } catch {
      // Fallback pra posição fixa se coordsAtPos falhar (ex: editor sem
      // foco). Adiciona jitter pra não empilhar caixas no mesmo ponto.
      const jitter = Math.random() * 1.5;
      wrap_x_cm = 4 + jitter;
      wrap_y_cm = 4 + jitter;
    }

    editor.commands.insertTextBox({
      wrap_mode: "in_front",
      wrap_x_cm,
      wrap_y_cm,
      width_cm: 6,
      height_cm: 2,
      border_enabled: true,
      border_color: "#1f2937",
      border_width: 1,
      border_style: "solid",
      fill_enabled: false,
      fill_color: "#ffffff",
    });
  };

  return (
    <button
      type="button"
      className={styles.btnLabel}
      onClick={handleClick}
      title="Inserir caixa de texto"
    >
      <TextCursorInput size={14} /> Caixa de texto
    </button>
  );
}

// Fórmula matemática — botão + diálogo MathLive (auto-contido). Insere no
// modo "bloco" por padrão; o diálogo permite "em linha". Um duplo-clique numa
// fórmula existente dispara `EDIT_MATH_EVENT`: abrimos o diálogo em modo edição
// pro nó selecionado.
function InsertMathBtn({ editor }: { editor: Editor }) {
  const [ctx, setCtx] = useState<MathDialogContext | null>(null);

  useEffect(() => {
    const onEdit = () => {
      const sel = editor.state.selection;
      const node = sel instanceof NodeSelection ? sel.node : null;
      if (!node) return;
      if (node.type.name === "mathBlock" || node.type.name === "mathInline") {
        setCtx({
          mode: "edit",
          variant: node.type.name === "mathBlock" ? "block" : "inline",
          latex: (node.attrs["latex"] as string) || "",
        });
      }
    };
    window.addEventListener(EDIT_MATH_EVENT, onEdit);
    return () => window.removeEventListener(EDIT_MATH_EVENT, onEdit);
  }, [editor]);

  return (
    <>
      <button
        type="button"
        className={styles.btnLabel}
        onClick={() =>
          setCtx({ mode: "insert", variant: "block", latex: "" })
        }
        title="Inserir fórmula matemática"
      >
        <Sigma size={14} /> Fórmula
      </button>
      <MathEditDialog
        editor={editor}
        context={ctx}
        onClose={() => setCtx(null)}
      />
    </>
  );
}

/**
 * V2 — InsertRegistrationBlockBtn: insere o "bloco de registro" do
 * cabeçalho do laudo como uma TABELA 1×3 com borda (o retângulo do Word):
 *   col 1 → dados (Registrado em / Ref. REQ / BO)
 *   col 2 → número do laudo (centralizado, negrito)
 *   col 3 → folha
 * Os textos são placeholders editáveis. Insere no editor ATIVO (corpo ou,
 * quando editando o cabeçalho, o headerEditor). Usa `insertContent` com o
 * JSON da tabela (cada célula precisa de ≥1 parágrafo — `block+`).
 */
function InsertRegistrationBlockBtn({ editor }: { editor: Editor }) {
  const handleClick = () => {
    const para = (
      text: string,
      opts: { align?: string; bold?: boolean } = {},
    ) => {
      const node: Record<string, unknown> = { type: "paragraph" };
      if (opts.align) node.attrs = { textAlign: opts.align };
      if (text) {
        const t: Record<string, unknown> = { type: "text", text };
        if (opts.bold) t.marks = [{ type: "bold" }];
        node.content = [t];
      }
      return node;
    };
    // V2/F1.2 — colwidths explícitos (larga / média / estreita, estilo Word).
    // Com `table-layout: fixed`, sem isto as 3 colunas ficariam iguais e o
    // bloco de registro regrediria o visual. `borderStyle: "none"` mantém só
    // o retângulo externo (sem grade interna), como o timbre do Word.
    const widths = registrationBlockColWidths();
    const cell = (paras: Record<string, unknown>[], colIdx: number) => ({
      type: "tableCell",
      attrs: { colspan: 1, rowspan: 1, colwidth: [widths[colIdx]] },
      content: paras,
    });
    const table = {
      type: "table",
      attrs: { id: generateTableId(), borderStyle: "none" },
      content: [
        {
          type: "tableRow",
          content: [
            cell(
              [
                para("Registrado em ___/___/______."),
                para("Ref. a REQ. nº ______"),
                para("BO nº ______"),
              ],
              0,
            ),
            cell(
              [
                para("LAUDO N° ______", { align: "center", bold: true }),
                para("______/DC/PCA", { align: "center", bold: true }),
              ],
              1,
            ),
            cell([para("Folha ___ de ___", { align: "center" })], 2),
          ],
        },
      ],
    };
    editor.chain().focus().insertContent(table).run();
  };

  return (
    <button
      type="button"
      className={styles.btnLabel}
      onClick={handleClick}
      title="Inserir bloco de registro (tabela 1×3 com borda) — ideal pro cabeçalho"
    >
      <TableIcon size={14} /> Bloco de registro
    </button>
  );
}
