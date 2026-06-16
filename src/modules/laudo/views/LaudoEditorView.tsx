/**
 * LaudoEditorView — orquestrador do editor TipTap + toolbar + inspector
 * + preview HTML + status bar.
 *
 * F2 — Edição rica:
 *   - Autosave debounced (3s) via `useAutosave`. Pode ser pausado quando
 *     o save manual está em andamento (evita race no `setState`).
 *   - Atalhos globais (Ctrl+S salva, Ctrl+F localizar, Ctrl+H substituir,
 *     Ctrl+P exporta PDF, Esc fecha barra de find).
 *   - Barra de Localizar/Substituir (`FindReplaceBar`) — montada sob a
 *     toolbar quando ativa.
 *   - Status bar inferior (`LaudoStatusBar`) com contagem de palavras,
 *     caracteres, parágrafos e indicador de save.
 *   - Indicador de save com 4 estados: salvo / salvando / dirty / erro.
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useEditor } from "@tiptap/react";
import type { JSONContent } from "@tiptap/core";
import { ArrowLeft } from "lucide-react";
import { EditorPage } from "../components/EditorPage";
import { EditorToolbar } from "../components/EditorToolbar";
import { EditorMenuBar } from "../components/EditorMenuBar";
import { Inspector } from "../components/Inspector";
import { HtmlPreview } from "../components/HtmlPreview";
import { FindReplaceBar } from "../components/FindReplaceBar";
import { KeyboardShortcutsDialog } from "../components/KeyboardShortcutsDialog";
import { LaudoErrorBoundary } from "../components/LaudoErrorBoundary";
import { SigdocsCoverHost } from "../components/SigdocsCoverHost";
import { LaudoStatusBar, type LaudoSaveState } from "../components/LaudoStatusBar";
import { PageControls } from "../components/PageControls";
import { useLaudoStore } from "../store/laudoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { useImageEditRoundtripStore } from "@stores/imageEditRoundtripStore";
import { useNavigate } from "react-router-dom";
import {
  A4_PAGE,
  applyLaudoStyle,
  joinWorkspace,
  laudoExtensions,
  marginsInCm,
  resolveEffectiveMargins,
  findInstitutionalTemplate,
  type LaudoStyleId,
  type SicroDoc,
} from "../document-engine";
import { useAutosave } from "../hooks/useAutosave";
import { useAutoBackup } from "../hooks/useAutoBackup";
import { useShortcuts } from "@core/useShortcuts";
import { useZoom } from "../hooks/useZoom";
import { useEditorMode } from "../hooks/useEditorMode";
import { useDragDropPhotos } from "../hooks/useDragDropPhotos";
import { usePasteImage } from "../hooks/usePasteImage";
import { useSelectedFigure } from "../hooks/useSelectedFigure";
// Q — Hook + overlay pra formas (rect, ellipse, arrow, line).
import { useSelectedShape } from "../hooks/useSelectedShape";
// Rodada 2 — hidrata o PNG das fórmulas importadas (OMML→LaTeX→PNG) ao abrir.
import { useMathHydration } from "../hooks/useMathHydration";
import { PhotoDropOverlay } from "../components/PhotoDropOverlay";
import { FigureOverlay } from "../components/FigureOverlay";
import { ShapeOverlay } from "../components/ShapeOverlay";
// U — TextBox overlay + hook.
import { useSelectedTextBox } from "../hooks/useSelectedTextBox";
import { TextBoxOverlay } from "../components/TextBoxOverlay";
// Overhaul tabelas (F2/F3/F4) — overlay + hook + diálogo de propriedades.
import { useSelectedTable } from "../hooks/useSelectedTable";
import { TableOverlay } from "../components/TableOverlay";
import { TablePropertiesDialog } from "../components/TablePropertiesDialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import { formatRelative } from "@core/formatters";
import { toSicroError } from "@core/errors";
import styles from "./LaudoEditorView.module.css";

interface LaudoEditorViewProps {
  workspacePath: string;
  onBack: () => void;
}

// F2 — Delay do autosave em ms. 3000 é o sweet spot: rápido o suficiente
// para o perito não perder trabalho, lento o suficiente para evitar
// thrashing em digitação contínua.
const AUTOSAVE_DELAY_MS = 3000;

export function LaudoEditorView({ workspacePath, onBack }: LaudoEditorViewProps) {
  const currentLaudo = useLaudoStore((s) => s.currentLaudo);
  const currentDoc = useLaudoStore((s) => s.currentDoc);
  const isSaving = useLaudoStore((s) => s.isMutating);
  const saveCurrent = useLaudoStore((s) => s.saveCurrent);
  const setHeader = useLaudoStore((s) => s.setHeader);
  const setFooter = useLaudoStore((s) => s.setFooter);
  const lastError = useLaudoStore((s) => s.lastError);
  // U1 — Pós-laudo: roteamento da toolbar pro editor ativo.
  // Quando `editingRegion === "header"`, comandos da toolbar (font size,
  // bold, cor, etc.) devem ir pro headerEditor; caso contrário, pro body.
  const editingRegion = useLaudoStore((s) => s.editingRegion);
  const activeOccurrence = useWorkspaceStore((s) => s.activeOccurrence);

  const [previewOpen, setPreviewOpen] = useState(false);
  const [liveContent, setLiveContent] = useState<JSONContent | null>(null);
  const [titleDraft, setTitleDraft] = useState(currentLaudo?.title ?? "");
  const [localError, setLocalError] = useState<string | null>(null);

  // F2 — Estado da barra de localizar.
  const [findOpen, setFindOpen] = useState(false);
  const [findShowReplace, setFindShowReplace] = useState(false);
  // F12.5 — Modal de atalhos (tecla ?)
  const [shortcutsOpen, setShortcutsOpen] = useState(false);

  // F3 — Zoom + modo do editor.
  const {
    zoom,
    setZoom,
    zoomIn,
    zoomOut,
    reset: resetZoom,
    fitWidth: fitZoomWidth,
    fitPage: fitZoomPage,
  } = useZoom(1);
  // `isEditable` é gerenciado por `EditorPage` via `editor.setEditable(mode)`,
  // então não precisamos consumir aqui.
  const { mode, setMode, showInspector, showStatusBar } =
    useEditorMode("edicao");
  // Ref do scroll container para medir e ajustar fit width/page.
  const editorRegionRef = useRef<HTMLDivElement>(null);

  // F2 — Estado do save (saved/saving/dirty/error) e timestamp do último save.
  const [saveState, setSaveState] = useState<LaudoSaveState>("saved");
  const [lastSavedAt, setLastSavedAt] = useState<Date | null>(null);
  // Re-renderiza a label "salvo há Xs" a cada 15s sem precisar de timer
  // dedicado para texto humanizado.
  const [, forceTick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => forceTick((n) => n + 1), 15_000);
    return () => clearInterval(t);
  }, []);

  // Keep titleDraft synced when the underlying laudo changes.
  useEffect(() => {
    setTitleDraft(currentLaudo?.title ?? "");
  }, [currentLaudo?.id, currentLaudo?.title]);

  const initialContent = currentDoc?.content ?? null;

  // F7.3 — Margens efetivas + orientação determinam dimensões do plugin
  // de paginação. Recriamos o editor quando essas dimensões mudam (ainda
  // que mudar margens via régua dispare reset; aceitamos isso porque
  // mudar margem é operação rara comparada a digitação).
  const paginationOpts = useMemo(() => {
    if (!currentDoc) return undefined;
    const template = findInstitutionalTemplate(
      currentDoc.layout?.institutional_template,
    );
    const margins = marginsInCm(resolveEffectiveMargins(currentDoc, template));
    const isLandscape = currentDoc.layout?.orientation === "landscape";
    const pageHeightCm = isLandscape ? A4_PAGE.widthCm : A4_PAGE.heightCm;
    return {
      pageHeightCm,
      marginTopCm: margins.top,
      marginBottomCm: margins.bottom,
      gapCm: 0.7,
    };
  }, [
    currentDoc?.layout?.page?.margins,
    currentDoc?.layout?.institutional_template,
    currentDoc?.layout?.orientation,
  ]);

  const editor = useEditor(
    {
      extensions: laudoExtensions({
        placeholder: "Comece a escrever o laudo ou insira uma seção…",
        pagination: paginationOpts,
      }),
      content: initialContent,
      editorProps: {
        attributes: {
          class: "sicro-editor-content",
          spellcheck: "true",
        },
      },
      autofocus: "end",
      onUpdate({ editor }) {
        setLiveContent(editor.getJSON());
        // F2 — qualquer update marca o documento como "dirty" até o autosave
        // (ou save manual) reverter para "saved".
        setSaveState((prev) => (prev === "saving" ? prev : "dirty"));
      },
    },
    // F7.3 — Só recriamos o editor quando o LAUDO muda (laudoId). Mudanças
    // de margem (via régua) são commitadas no `.sicrodoc` mas a paginação
    // do plugin continua usando os valores iniciais — para reaplicar com
    // as novas margens, o usuário precisa fechar e reabrir o laudo. Esse
    // trade-off evita resetar undo/cursor durante drag de margens.
    [currentLaudo?.id],
  );

  // O — Função compartilhada de inserção de fotos em UM editor TipTap.
  // Os fluxos (drag&drop body, paste body, paste header) deságuam aqui
  // depois do backend ter copiado o binário e devolvido
  // `ImportedPhoto[]`.
  //
  // `targetEditor`: o editor de destino — pode ser o body (`editor`)
  // ou o header (`headerEditor`). Não pegamos de fora pra evitar que o
  // header e o body briguem pelo mesmo callback.
  //
  // `insertPos` opcional: drag&drop passa a posição calculada via
  // `view.posAtCoords` (onde o mouse soltou); paste passa undefined
  // pra usar a seleção atual do cursor (onde o user pressionou Ctrl+V).
  const insertPhotosIntoEditor = useCallback(
    (
      targetEditor: import("@tiptap/react").Editor,
      photos: import("@domain/photo_drop").ImportedPhoto[],
      insertPos?: number,
    ) => {
      if (typeof insertPos === "number") {
        targetEditor.chain().focus().setTextSelection(insertPos).run();
      } else {
        targetEditor.chain().focus().run();
      }
      for (const photo of photos) {
        const absolutePath = joinWorkspace(
          workspacePath,
          photo.relative_path,
        );
        const src = convertFileSrc(absolutePath);
        const metadata = {
          imported_at: new Date().toISOString(),
          sha256: photo.sha256,
          width: photo.width,
          height: photo.height,
          mime: photo.mime,
          size_bytes: photo.size_bytes,
          date_taken: photo.date_taken,
          exif: photo.exif_json ? safeJsonParse(photo.exif_json) : null,
        };
        // Caption inicial = nome do arquivo sem extensão. O user troca
        // por algo mais significativo depois (ou deixa, o auto-numbering
        // já adiciona "Figura N — …" via decoration).
        const captionDefault =
          photo.original_filename.replace(/\.[^.]+$/, "") ||
          "Descrição da figura.";
        targetEditor
          .chain()
          .focus()
          .insertFigure({
            src,
            alt: photo.original_filename,
            kind: "image",
            width: "70%",
            align: "center",
            relative_path: photo.relative_path,
            source_hash: photo.sha256,
            metadata_json: JSON.stringify(metadata),
            caption: captionDefault,
          })
          .run();
      }
    },
    [workspacePath],
  );

  // O — Drag & drop de fotos: hook registra listener do Tauri
  // `onDragDropEvent`. No drop bem-sucedido, insere `figure` nodes no
  // editor na posição do mouse via `view.posAtCoords`. Plugado APÓS o
  // useEditor pra ter acesso à instância.
  //
  // Usa o command oficial `insertFigure` em vez de montar a estrutura
  // crua. Razões:
  //  1. ProseMirror não aceita text nodes vazios — figcaption com
  //     `text: ""` quebrava o insert silenciosamente.
  //  2. O command cuida de gerar um UUID estável (`id` attr),
  //     necessário pra cross-references e auto-numeração (F12.1).
  const dragDrop = useDragDropPhotos({
    enabled: !!editor && mode !== "leitura",
    workspacePath,
    laudoId: currentLaudo?.id ?? null,
    onImported: (photos, dropPos) => {
      if (!editor) return;
      const view = editor.view;
      const posInfo = view.posAtCoords({
        left: dropPos.x,
        top: dropPos.y,
      });
      const insertPos = posInfo?.pos ?? editor.state.doc.content.size;
      insertPhotosIntoEditor(editor, photos, insertPos);
      dragDrop.reset();
    },
    onErrors: (errors) => {
      // eslint-disable-next-line no-console
      console.warn("[photo-drop] errors", errors);
      setLocalError(
        `Algumas fotos falharam: ${errors
          .slice(0, 3)
          .map((e) => e.reason)
          .join(", ")}`,
      );
    },
  });

  // T — Paste (Ctrl+V) de fotos: hook attacha listener no
  // `editor.view.dom` (capture phase, antes do ProseMirror) e
  // intercepta paste com imagens — bitmap raw (screenshot/browser) ou
  // `File` do Explorer copy. Bytes vão pro backend em base64; o backend
  // reusa o mesmo pipeline do drop. Inserção na seleção atual (cursor
  // onde o user pressionou Ctrl+V).
  const pasteImage = usePasteImage({
    enabled: !!editor && mode !== "leitura",
    editor,
    workspacePath,
    laudoId: currentLaudo?.id ?? null,
    onImported: (photos) => {
      if (!editor) return;
      // Sem insertPos: usa seleção atual do cursor.
      insertPhotosIntoEditor(editor, photos);
      pasteImage.reset();
    },
    onErrors: (errors) => {
      // eslint-disable-next-line no-console
      console.warn("[paste-image] errors", errors);
      setLocalError(
        `Algumas fotos falharam: ${errors
          .slice(0, 3)
          .map((e) => e.reason)
          .join(", ")}`,
      );
    },
  });

  // P — Detecta se há um figure selecionado pra renderizar handles.
  const selectedFigure = useSelectedFigure(editor);
  const selectedShape = useSelectedShape(editor);
  // Rodada 2 — gera o PNG das fórmulas importadas (sem render_png) ao carregar.
  useMathHydration(editor);
  // U — Detecta TextBox selecionada (body + header) pra renderizar overlay.
  const selectedTextBox = useSelectedTextBox(editor);
  // Overhaul tabelas — detecta a tabela que contém o cursor (corpo).
  const selectedTable = useSelectedTable(editor);
  // Diálogo de propriedades da tabela: aberto pela toolbar/menu do overlay.
  // Guarda QUAL editor abriu (corpo/cabeçalho/rodapé) pra aplicar no certo.
  const [tablePropsEditor, setTablePropsEditor] = useState<
    import("@tiptap/react").Editor | null
  >(null);

  // Pós-laudo S — referência opcional ao editor TipTap do cabeçalho.
  // EditorPage instancia o `headerEditor` internamente via `useHeaderEditor`;
  // ele é entregue aqui via `onHeaderEditorReady` pra que possamos atachar
  // uma segunda FigureOverlay quando o cursor estiver editando uma figure
  // no header. Quando nada de figure está selecionado no header, o hook
  // retorna null e a overlay vira no-op.
  const [headerEditor, setHeaderEditor] = useState<
    import("@tiptap/react").Editor | null
  >(null);
  const selectedHeaderFigure = useSelectedFigure(headerEditor);
  // U — TextBox no header também (mesma instrumentação).
  const selectedHeaderTextBox = useSelectedTextBox(headerEditor);
  // Q — Forma (shape) no header também. Sem isto, uma forma inserida no
  // cabeçalho ficava SEM overlay (handles) — entrava no doc mas não dava pra
  // selecionar/mover/girar. A inserção já mirava o headerEditor (toolbar U1);
  // faltava só o overlay espelhado, como o TextBox já tinha.
  const selectedHeaderShape = useSelectedShape(headerEditor);
  // Tabela no cabeçalho (mesma instrumentação — bloco de registro etc.).
  const selectedHeaderTable = useSelectedTable(headerEditor);

  // W (fase 2b) — referência opcional ao editor TipTap do RODAPÉ, simétrico
  // ao headerEditor. EditorPage instancia via `useFooterEditor` e entrega
  // aqui por `onFooterEditorReady`; assim podemos atachar FigureOverlay /
  // TextBoxOverlay quando o cursor estiver editando o rodapé (ex.: o brasão
  // da Polícia Científica importado do .docx).
  const [footerEditor, setFooterEditor] = useState<
    import("@tiptap/react").Editor | null
  >(null);
  const selectedFooterFigure = useSelectedFigure(footerEditor);
  const selectedFooterTextBox = useSelectedTextBox(footerEditor);
  // Tabela no rodapé (simétrico ao cabeçalho).
  const selectedFooterTable = useSelectedTable(footerEditor);

  // W — Paste de fotos NO RODAPÉ (mesma razão do headerPasteImage: o evento
  // `paste` cai no DOM da instância TipTap do rodapé).
  const footerPasteImage = usePasteImage({
    enabled: !!footerEditor && mode !== "leitura",
    editor: footerEditor,
    workspacePath,
    laudoId: currentLaudo?.id ?? null,
    onImported: (photos) => {
      if (!footerEditor) return;
      insertPhotosIntoEditor(footerEditor, photos);
      footerPasteImage.reset();
    },
    onErrors: (errors) => {
      // eslint-disable-next-line no-console
      console.warn("[paste-image:footer] errors", errors);
      setLocalError(
        `Algumas fotos do rodapé falharam: ${errors
          .slice(0, 3)
          .map((e) => e.reason)
          .join(", ")}`,
      );
    },
  });

  // T — Paste de fotos NO CABEÇALHO. O headerEditor é uma instância
  // TipTap separada (gerenciada pelo `useHeaderEditor` dentro do
  // EditorPage). Quando o cursor tá no header, o evento `paste` cai
  // no DOM dele — não no do body — então precisamos atachar um
  // listener próprio aqui, senão o ProseMirror do header faz default
  // insertion (img inerte sem nossos attrs/handles).
  //
  // Mesmo callback de inserção, só que mira o headerEditor.
  const headerPasteImage = usePasteImage({
    enabled: !!headerEditor && mode !== "leitura",
    editor: headerEditor,
    workspacePath,
    laudoId: currentLaudo?.id ?? null,
    onImported: (photos) => {
      if (!headerEditor) return;
      insertPhotosIntoEditor(headerEditor, photos);
      headerPasteImage.reset();
    },
    onErrors: (errors) => {
      // eslint-disable-next-line no-console
      console.warn("[paste-image:header] errors", errors);
      setLocalError(
        `Algumas fotos do cabeçalho falharam: ${errors
          .slice(0, 3)
          .map((e) => e.reason)
          .join(", ")}`,
      );
    },
  });

  // ─────────────────────────────────────────────────────────────────
  // Pós-laudo S — Round-trip Laudo ↔ Editor de Imagem.
  //
  // 1. Quando o perito clica "Editar" numa foto, `handleEditPhoto` seta
  //    a request no store de roundtrip e navega para `/imagem`.
  // 2. Quando o ImageEditor termina (Salvar e voltar), o store recebe
  //    o output path; aqui detectamos `state === "completed"` e
  //    aplicamos o novo path em TODA figure que apontava para a foto
  //    original (tipicamente uma só).
  // ─────────────────────────────────────────────────────────────────
  const navigate = useNavigate();
  const startEditRoundtrip = useImageEditRoundtripStore((s) => s.startEdit);
  const roundtripState = useImageEditRoundtripStore((s) => s.state);
  const roundtripResult = useImageEditRoundtripStore((s) => s.result);
  const clearRoundtrip = useImageEditRoundtripStore((s) => s.clearRoundtrip);

  const handleEditPhoto = useCallback(
    async (relativePath: string) => {
      if (!relativePath) return;
      // CRÍTICO: Força save do estado atual do editor ANTES de navegar.
      // Sem isso, se o usuário arrastou a foto e clicou EDITAR antes do
      // autosave (debounced 3s), a foto está só na memória do editor
      // (liveContent), mas o currentDoc do store ainda tem o estado
      // anterior (sem a foto). Quando voltamos do /imagem, o LaudoEditorView
      // remonta lendo currentDoc (estale) → editor sem a foto.
      if (editor) {
        try {
          await saveCurrent(workspacePath, editor.getJSON());
        } catch {
          // Se o save falhar, ainda navegamos — round-trip falhará
          // graciosamente via o timeout de 5s.
        }
      }
      startEditRoundtrip({
        workspace_path: workspacePath,
        source_relative_path: relativePath,
        laudo_id: currentLaudo?.id ?? null,
        laudo_title: currentLaudo?.title ?? null,
      });
      navigate("/imagem");
    },
    [
      workspacePath,
      currentLaudo?.id,
      currentLaudo?.title,
      navigate,
      startEditRoundtrip,
      editor,
      saveCurrent,
    ],
  );

  // Aplica o resultado da edição: walk no doc, troca `src` + `relative_path`
  // de todo Figure node cujo path bate com o source original. Persiste
  // automaticamente via o onUpdate do editor.
  //
  // IMPORTANTE: O Figure NodeView renderiza `<img src={node.attrs.src}>`
  // direto — sem chamar `convertFileSrc` em runtime. O fluxo padrão
  // do laudoStore é (MVP 4):
  //   - no `save`:   normaliza `src` → guarda só `relative_path`
  //   - no `load`:   resolve `relative_path` → `src = convertFileSrc(joinWorkspace(...))`
  // Como nossa atualização é em runtime (não passa por load/save),
  // precisamos fazer a transformação à mão: setamos `relative_path`
  // pro path cru E `src` pra URL do asset protocol do Tauri. Quando o
  // doc for salvo depois, o normalize stripa o `src` e mantém só
  // `relative_path` — sem dano.
  //
  // Match também tolera o caso em que `attrs.src` ainda é o convertFileSrc
  // do path antigo (compara contra a URL completa, não só o path cru).
  useEffect(() => {
    if (roundtripState !== "completed" || !roundtripResult || !editor) return;
    const { source_relative_path, output_relative_path } = roundtripResult;
    const sourceResolvedSrc = (() => {
      try {
        return convertFileSrc(joinWorkspace(workspacePath, source_relative_path));
      } catch {
        return null;
      }
    })();
    const outputResolvedSrc = (() => {
      try {
        return convertFileSrc(joinWorkspace(workspacePath, output_relative_path));
      } catch {
        return output_relative_path;
      }
    })();

    // Timeout de 5s pra dar up. Se a figure não for encontrada em 5s,
    // muito provavelmente ela foi removida do laudo (não tem o que
    // patchar). Limpa o store pra não ficar bloqueado eternamente.
    const giveUpAt = Date.now() + 5000;

    let applied = false;
    const tryApply = () => {
      if (applied) return;
      const updates: Array<{ pos: number; attrs: Record<string, unknown> }> = [];
      editor.state.doc.descendants((node, pos) => {
        if (node.type.name !== "figure") return;
        const attrs = node.attrs as { relative_path?: string; src?: string };
        const matches =
          attrs.relative_path === source_relative_path ||
          attrs.src === source_relative_path ||
          (sourceResolvedSrc !== null && attrs.src === sourceResolvedSrc);
        if (!matches) return;
        updates.push({
          pos,
          attrs: {
            ...node.attrs,
            relative_path: output_relative_path,
            src: outputResolvedSrc,
          },
        });
      });
      if (updates.length === 0) {
        // Sem match. Se passou do timeout, desiste pra liberar o store.
        if (Date.now() > giveUpAt) {
          applied = true;
          console.warn(
            "[roundtrip] Figure não encontrada após 5s — limpando store.",
            "A foto pode ter sido removida do laudo. Re-insira a foto " +
              "manualmente e o round-trip da próxima edição funcionará.",
          );
          clearRoundtrip();
        }
        return;
      }
      const tr = editor.state.tr;
      for (const u of updates) {
        tr.setNodeMarkup(u.pos, undefined, u.attrs);
      }
      editor.view.dispatch(tr);
      applied = true;
      clearRoundtrip();
    };

    // Primeira tentativa imediata — pode falhar se o doc ainda está vazio
    // por race condition (LaudoEditorView monta antes do setContent do
    // currentDoc terminar).
    tryApply();

    // Fallback: escuta TODA transação do editor e re-tenta. Guard
    // `applied` curto-circuita após sucesso, então não há custo.
    if (!applied) {
      const handler = () => tryApply();
      editor.on("transaction", handler);
      return () => {
        editor.off("transaction", handler);
      };
    }
    return undefined;
  }, [roundtripState, roundtripResult, editor, clearRoundtrip, workspacePath]);

  // Initialize liveContent the first time the editor is ready.
  const initializedRef = useRef(false);
  useEffect(() => {
    if (editor && !initializedRef.current && initialContent) {
      setLiveContent(editor.getJSON());
      initializedRef.current = true;
      // Documento recém-aberto, ainda não foi modificado.
      setSaveState("saved");
      setLastSavedAt(
        currentLaudo?.updated_at ? new Date(currentLaudo.updated_at) : null,
      );
    }
  }, [editor, initialContent, currentLaudo?.updated_at]);

  // Pós-laudo S — Hidratação do corpo na CORRIDA DE MONTAGEM.
  //
  // Quando navegamos /imagem→/laudo o LaudoEditorView remonta e o useEditor
  // pode criar o editor com initialContent=null (o store carrega o doc async).
  // Quando o currentDoc chega, o editor já existe mas está vazio — então
  // injetamos o conteúdo do doc UMA única vez (senão o round-trip do crop
  // não acha as figures, etc.).
  //
  // BUG corrigido: a versão antiga re-injetava o currentDoc sempre que o
  // corpo ficava vazio. Como "vazio" também inclui o usuário ter APAGADO
  // tudo, ao re-renderizar (ex.: clicar no cabeçalho) o texto apagado
  // "ressuscitava" a partir do currentDoc — que fica atrás do editor ao vivo
  // (liveContent). A guarda `bodyHydrationDoneRef` limita a hidratação
  // automática à janela de montagem (uma vez por laudo); depois disso, um
  // corpo vazio é intenção do usuário e nunca é sobrescrito.
  const bodyHydrationDoneRef = useRef(false);

  // Rearma a hidratação ao trocar de laudo (o useEditor recria o editor).
  useEffect(() => {
    bodyHydrationDoneRef.current = false;
  }, [currentLaudo?.id]);

  useEffect(() => {
    if (bodyHydrationDoneRef.current) return;
    // Espera editor E doc prontos. Enquanto o doc não chega (null) seguimos
    // armados — é exatamente a corrida de montagem que queremos cobrir.
    if (!editor || !currentDoc) return;
    const editorEmpty = editor.state.doc.content.size <= 4;
    if (editorEmpty && currentDoc.content) {
      // Editor montou vazio antes do doc chegar → injeta o conteúdo do doc.
      // emitUpdate:false: carregar conteúdo não deve marcar "dirty" nem
      // disparar onUpdate; sincronizamos o liveContent manualmente.
      editor.commands.setContent(currentDoc.content, { emitUpdate: false });
      setLiveContent(editor.getJSON());
    }
    // Editor + doc prontos = corrida resolvida. Nunca mais auto-hidratar
    // este laudo (senão apagar tudo + re-render ressuscita o texto).
    bodyHydrationDoneRef.current = true;
  }, [editor, currentDoc]);

  // Build the SicroDoc snapshot the inspector + preview will look at.
  const docForInspector: SicroDoc | null = useMemo(() => {
    if (!currentDoc) return null;
    return liveContent
      ? { ...currentDoc, content: liveContent }
      : currentDoc;
  }, [currentDoc, liveContent]);

  // F2 — função de save central, usada por:
  //   - botão Salvar (toolbar / atalho Ctrl+S);
  //   - autosave debounced;
  //   - handleTitleCommit (mudança de título).
  const persist = useCallback(
    async (content: JSONContent): Promise<boolean> => {
      try {
        setSaveState("saving");
        await saveCurrent(workspacePath, content);
        setSaveState("saved");
        setLastSavedAt(new Date());
        setLocalError(null);
        return true;
      } catch (err) {
        setSaveState("error");
        setLocalError(toSicroError(err).message);
        return false;
      }
    },
    [saveCurrent, workspacePath],
  );

  // F12.4 — Auto-backup em background no IndexedDB (cross-session).
  useAutoBackup({
    editor,
    laudoId: currentLaudo?.id ?? null,
    intervalMs: 30_000,
  });

  // F12.5 — A tecla `?` (abre a lista de atalhos) é ligada via `useShortcuts`
  // mais abaixo, no grupo de teclas sem modificador (não dispara enquanto o
  // perito digita no editor).

  // F2 — autosave: dispara `persist` após 3s sem digitação.
  // Pausa enquanto há save manual em andamento (evita conflito de promises).
  useAutosave({
    editor,
    saveFn: persist,
    delayMs: AUTOSAVE_DELAY_MS,
    paused: isSaving,
    enabled: !!currentLaudo,
    onError: (err) => {
      // Já cobrimos via setSaveState("error") dentro de persist; mantemos
      // log defensivo aqui caso a falha aconteça fora do persist.
      console.warn("[laudo] autosave error", err);
    },
  });

  const handleSave = useCallback(async () => {
    if (!editor) return;
    // F12.10 — Toast progressivo durante save manual (Ctrl+S).
    //   Autosave (3s debounce) NÃO dispara toast — fica só no indicador
    //   do status bar, pra não poluir. Manual save mostra feedback.
    const { pushToast, dismissToast } = await import(
      "@/components/toast/toastStore"
    );
    const toastId = pushToast("progress", "Salvando laudo…", {
      title: "Salvar",
    });
    try {
      await persist(editor.getJSON());
      dismissToast(toastId);
      pushToast("success", "Laudo salvo com sucesso.");
    } catch (err) {
      dismissToast(toastId);
      pushToast("error", String((err as Error)?.message ?? err), {
        title: "Erro ao salvar",
      });
    }
  }, [editor, persist]);

  // Atalho Ctrl+P — Exporta PDF de nível de app (mesmo pipeline do ExportMenu),
  // com feedback por toast e abrindo o Explorer na pasta do arquivo gerado.
  const handleExportPdf = useCallback(async () => {
    if (!editor || !currentDoc) return;
    const { pushToast, dismissToast } = await import(
      "@/components/toast/toastStore"
    );
    const toastId = pushToast("progress", "Exportando PDF…", {
      title: "Exportação",
    });
    try {
      // Garante que o conteúdo em tela esteja persistido antes de renderizar.
      const content = editor.getJSON();
      const docToExport: SicroDoc = { ...currentDoc, content };
      const { exportLaudo } = await import("../services/laudoExport");
      const { export: result } = await exportLaudo(
        "pdf",
        workspacePath,
        currentLaudo?.id ?? "",
        docToExport,
        (activeOccurrence as unknown as Record<string, unknown> | null) ?? null,
        { revealAfter: true },
      );
      dismissToast(toastId);
      pushToast(
        "success",
        `PDF gerado em ${result.relative_path} — pasta aberta no Explorer.`,
        { title: "Exportação concluída", durationMs: 6000 },
      );
    } catch (err) {
      dismissToast(toastId);
      pushToast("error", toSicroError(err).message, {
        title: "Falha na exportação",
      });
    }
  }, [editor, currentDoc, currentLaudo?.id, workspacePath, activeOccurrence]);

  // F3 — handlers de fit usando dimensões reais do scroll container.
  // Pagamos um custo de medição on-demand para evitar dependência de
  // observer; é só na hora do clique.
  const handleFitWidth = useCallback(() => {
    const el = editorRegionRef.current;
    const pageWidthCm =
      currentDoc?.layout?.orientation === "landscape"
        ? A4_PAGE.heightCm
        : A4_PAGE.widthCm;
    if (el) fitZoomWidth(el.clientWidth, pageWidthCm);
  }, [currentDoc?.layout?.orientation, fitZoomWidth]);
  const handleFitPage = useCallback(() => {
    const el = editorRegionRef.current;
    const isLandscape = currentDoc?.layout?.orientation === "landscape";
    const pageWidthCm = isLandscape ? A4_PAGE.heightCm : A4_PAGE.widthCm;
    const pageHeightCm = isLandscape ? A4_PAGE.widthCm : A4_PAGE.heightCm;
    if (el)
      fitZoomPage(el.clientWidth, el.clientHeight, pageWidthCm, pageHeightCm);
  }, [currentDoc?.layout?.orientation, fitZoomPage]);

  // F3.1 — Handler de mudança de orientação direto da status bar.
  // Persiste via `updateLayout`, igual ao Inspector "Página" — single
  // source of truth no `doc.layout.orientation`.
  const updateLayout = useLaudoStore((s) => s.updateLayout);
  const handleOrientationChange = useCallback(
    async (next: "portrait" | "landscape") => {
      if (!currentDoc) return;
      if ((currentDoc.layout?.orientation ?? "portrait") === next) return;
      try {
        await updateLayout(workspacePath, { orientation: next });
      } catch (err) {
        setLocalError(toSicroError(err).message);
      }
    },
    [currentDoc, updateLayout, workspacePath],
  );

  // Atalhos de nível de aplicação, customizáveis em Configurações (escopo
  // `laudo`). Os atalhos INTERNOS do TipTap (Ctrl+B/I/U, Ctrl+Z/Y,
  // Ctrl+Shift+X, etc.) NÃO passam por aqui — o editor os trata sozinho.
  //
  // Divididos em DOIS grupos por causa do foco no editor (contentEditable):
  //   1. Combos com modificador (Ctrl / Ctrl+Alt) + Esc → precisam disparar
  //      MESMO com o cursor dentro do texto, então usam `allowInInputs`.
  //   2. Teclas sem modificador (ex.: `?`) → NÃO devem disparar enquanto o
  //      perito digita, então usam o guard padrão de inputs.
  const applyStyleById = useCallback(
    (id: LaudoStyleId) => {
      if (editor) applyLaudoStyle(editor, id);
    },
    [editor],
  );

  // Grupo 1 — combos com modificador (e Esc), válidos mesmo digitando.
  useShortcuts(
    {
      "laudo.save": () => void handleSave(),
      "laudo.exportPdf": () => void handleExportPdf(),
      "laudo.find": () => {
        setFindShowReplace(false);
        setFindOpen(true);
      },
      "laudo.replace": () => {
        setFindShowReplace(true);
        setFindOpen(true);
      },
      // F3.1 — Esc: 1) fecha localizar; 2) sai do modo Foco; 3) no-op.
      "laudo.escape": () => {
        if (findOpen) {
          setFindOpen(false);
        } else if (mode === "foco") {
          setMode("edicao");
        }
      },
      // F4 — estilos documentais (applyLaudoStyle, não headings nativos).
      "laudo.style.normal": () => applyStyleById("normal"),
      "laudo.style.titulo1": () => applyStyleById("titulo_1"),
      "laudo.style.titulo2": () => applyStyleById("titulo_2"),
      "laudo.style.titulo3": () => applyStyleById("titulo_3"),
      "laudo.style.subtitulo": () => applyStyleById("subtitulo"),
      "laudo.style.secaoTecnica": () => applyStyleById("secao_tecnica"),
      "laudo.style.quesito": () => applyStyleById("quesito"),
      "laudo.style.resposta": () => applyStyleById("resposta"),
      // F3 — zoom (CSS scale).
      "laudo.zoomIn": zoomIn,
      "laudo.zoomOut": zoomOut,
      "laudo.zoomReset": resetZoom,
      "laudo.fitWidth": handleFitWidth,
      "laudo.fitPage": handleFitPage,
      "laudo.togglePreview": () => setPreviewOpen((v) => !v),
      // Modos do editor.
      "laudo.mode.edicao": () => setMode("edicao"),
      "laudo.mode.leitura": () => setMode("leitura"),
      "laudo.mode.foco": () => setMode("foco"),
      "laudo.mode.revisao": () => setMode("revisao"),
    },
    { enabled: !!editor, allowInInputs: true },
  );

  // Grupo 2 — teclas sem modificador (não disparam enquanto digita).
  useShortcuts(
    {
      "laudo.help": () => setShortcutsOpen(true),
    },
    { enabled: !!editor },
  );

  if (!currentLaudo || !currentDoc || !editor || !docForInspector) {
    return (
      <div className={styles.root}>
        <div className={styles.headerRow}>
          <button type="button" className={styles.backBtn} onClick={onBack}>
            <ArrowLeft size={14} /> Voltar
          </button>
        </div>
        <div style={{ padding: "var(--space-8)" }}>
          <p style={{ color: "var(--sicro-fg-muted)" }}>Carregando laudo…</p>
        </div>
      </div>
    );
  }

  const handleTitleCommit = async () => {
    const trimmed = titleDraft.trim();
    if (!trimmed || trimmed === currentLaudo.title) return;
    try {
      const content = editor.getJSON();
      const nextDoc: SicroDoc = { ...currentDoc, title: trimmed, content };
      // saveCurrent writes envelope using currentDoc as previous; we patch it
      // first by stashing it back into the store, then saving.
      useLaudoStore.setState({ currentDoc: nextDoc });
      await persist(content);
    } catch (err) {
      setLocalError(toSicroError(err).message);
    }
  };

  const saveLabel = lastSavedAt
    ? `Salvo ${formatRelative(lastSavedAt.toISOString())}`
    : "Salvo";

  return (
    <div className={styles.root}>
      <div className={styles.headerRow}>
        <button type="button" className={styles.backBtn} onClick={onBack}>
          <ArrowLeft size={14} /> Voltar
        </button>
        <input
          className={styles.titleInput}
          value={titleDraft}
          onChange={(e) => setTitleDraft(e.target.value)}
          onBlur={handleTitleCommit}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              (e.target as HTMLInputElement).blur();
            }
          }}
          placeholder="Título do laudo"
          aria-label="Título do laudo"
        />
        <span className={styles.savedBadge}>
          atualizado {formatRelative(currentLaudo.updated_at)}
        </span>
      </div>

      {/* F4.1 — Menu superior com popovers de configuração
           (Validações / Estilos / Cabeçalho / Página / Dados).
           Substitui as 5 abas extras que estavam no Inspector lateral. */}
      <EditorMenuBar
        doc={docForInspector}
        editor={editor}
        activeEditor={
          editingRegion === "header" && headerEditor
            ? headerEditor
            : editingRegion === "footer" && footerEditor
              ? footerEditor
              : editor
        }
      />

      <EditorToolbar
        editor={
          // U1 — Quando o user tá editando o header, a toolbar precisa
          // mirar o headerEditor pra que font-size, bold, cor, etc.
          // afetem o cabeçalho. W — idem pro footerEditor. Fallback pro
          // body quando a instância da região ainda não foi montada.
          editingRegion === "header" && headerEditor
            ? headerEditor
            : editingRegion === "footer" && footerEditor
              ? footerEditor
              : editor
        }
        isSaving={isSaving}
        isPreviewOpen={previewOpen}
        onSave={handleSave}
        onTogglePreview={() => setPreviewOpen((v) => !v)}
        onOpenFind={() => {
          setFindShowReplace(false);
          setFindOpen(true);
        }}
        workspacePath={workspacePath}
        laudoId={currentLaudo.id}
        laudoTitle={currentLaudo.title}
        doc={docForInspector}
        occurrence={
          activeOccurrence as unknown as Record<string, unknown> | null
        }
      />

      {findOpen && (
        <FindReplaceBar
          editor={editor}
          showReplace={findShowReplace}
          onClose={() => setFindOpen(false)}
        />
      )}

      {/* F12.5 — Modal global de atalhos (abre com tecla `?`) */}
      <KeyboardShortcutsDialog
        open={shortcutsOpen}
        onClose={() => setShortcutsOpen(false)}
      />

      {(localError || lastError?.message) && (
        <div className={styles.errorBanner}>{localError ?? lastError?.message}</div>
      )}

      {/* F12.6 — Boundary global isola crashes do TipTap/ProseMirror */}
      <LaudoErrorBoundary
        laudoId={currentLaudo.id}
        onBack={onBack}
        onRestoreBackup={(entry) => {
          // Persiste o conteúdo do auto-backup como conteúdo atual.
          // O editor irá re-inicializar com esse conteúdo no remount.
          void saveCurrent(workspacePath, entry.content).catch((err) =>
            setLocalError(toSicroError(err).message),
          );
        }}
      >
        <div className={styles.body} data-sigdocs-cover-body="1">
          {/* J — Cover do SIGDOC: quando ativo, este host invisível
              cobre toda a área do .body. O webview borderless do
              backend é posicionado em cima desta área (mesma bounding
              rect via ResizeObserver). */}
          <SigdocsCoverHost />
          <div className={styles.editorRegion} ref={editorRegionRef}>
            <EditorPage
              editor={editor}
              doc={docForInspector}
              occurrence={activeOccurrence}
              zoom={zoom}
              mode={mode}
              onZoomIn={zoomIn}
              onZoomOut={zoomOut}
              onSetZoom={setZoom}
              onExitFoco={() => setMode("edicao")}
              onLayoutChange={(patch) =>
                void updateLayout(workspacePath, patch).catch((err) =>
                  setLocalError(toSicroError(err).message),
                )
              }
              onHeaderChange={(next) =>
                void setHeader(workspacePath, next).catch((err) =>
                  setLocalError(toSicroError(err).message),
                )
              }
              onHeaderEditorReady={setHeaderEditor}
              onFooterChange={(next) =>
                void setFooter(workspacePath, next).catch((err) =>
                  setLocalError(toSicroError(err).message),
                )
              }
              onFooterEditorReady={setFooterEditor}
            />
            {previewOpen && (
              <HtmlPreview
                doc={docForInspector}
                liveContent={liveContent}
                occurrence={activeOccurrence}
                workspacePath={workspacePath}
                onClose={() => setPreviewOpen(false)}
              />
            )}
            {/* O — Overlay do drag-and-drop de fotos. Aparece sobre o
                editor quando o user arrasta arquivos do Windows Explorer.
                Posição é coberta pelo `position: relative` do .editorRegion.
                T — Reaproveitado pra paste (body) e paste (header).
                Quando o usuário cola uma foto via Ctrl+V, o overlay
                mostra "Importando…" durante a transferência base64 →
                backend → workspace. Prioridade: drag > body paste >
                header paste (a sobreposição entre os três é rara). */}
            <PhotoDropOverlay
              state={
                dragDrop.state !== "idle"
                  ? dragDrop.state
                  : pasteImage.state !== "idle"
                    ? pasteImage.state
                    : headerPasteImage.state !== "idle"
                      ? headerPasteImage.state
                      : footerPasteImage.state
              }
              errorMessage={
                dragDrop.state !== "idle"
                  ? dragDrop.errorMessage
                  : pasteImage.state !== "idle"
                    ? pasteImage.errorMessage
                    : headerPasteImage.state !== "idle"
                      ? headerPasteImage.errorMessage
                      : footerPasteImage.errorMessage
              }
            />
            {/* P — Overlay com handles de resize/rotate + floating toolbar
                pra figura selecionada. Renderiza nada quando não há
                figure selecionado. */}
            <FigureOverlay
              editor={editor}
              selected={selectedFigure}
              containerRef={editorRegionRef}
              onEditPhoto={handleEditPhoto}
            />
            {/* Pós-laudo S — Mesma overlay, mas atrelada ao editor do
                cabeçalho. Quando o perito clica numa foto inserida no
                header, a overlay aparece com handles de resize/rotate +
                wrap modes, exatamente como no corpo do texto. Só
                renderiza enquanto há figure selecionada lá. */}
            {headerEditor && (
              <FigureOverlay
                editor={headerEditor}
                selected={selectedHeaderFigure}
                containerRef={editorRegionRef}
                onEditPhoto={handleEditPhoto}
              />
            )}
            {/* W (fase 2b) — Mesma overlay pra figuras do RODAPÉ (ex.: o
                brasão da Polícia Científica que vem do .docx). Só
                renderiza enquanto há figure selecionada no footerEditor. */}
            {footerEditor && (
              <FigureOverlay
                editor={footerEditor}
                selected={selectedFooterFigure}
                containerRef={editorRegionRef}
                onEditPhoto={handleEditPhoto}
              />
            )}
            {/* Q — Overlay com handles + toolbar pra forma selecionada
                (rect, ellipse, arrow, line). Renderiza nada quando não
                há shape selecionada. */}
            <ShapeOverlay
              editor={editor}
              selected={selectedShape}
              containerRef={editorRegionRef}
            />
            {headerEditor && (
              <ShapeOverlay
                editor={headerEditor}
                selected={selectedHeaderShape}
                containerRef={editorRegionRef}
              />
            )}
            {/* U — TextBox overlay: handles + toolbar com controles de
                border/fill/rotação. Body + header (2 instâncias). */}
            <TextBoxOverlay
              editor={editor}
              selected={selectedTextBox}
              containerRef={editorRegionRef}
            />
            {headerEditor && (
              <TextBoxOverlay
                editor={headerEditor}
                selected={selectedHeaderTextBox}
                containerRef={editorRegionRef}
                onBeforeEnterTextEdit={() => {
                  // U fix — header textbox dblclick ativa header mode
                  // pra editor ficar editable, senão a TextSelection é
                  // setada num editor read-only e nada acontece.
                  useLaudoStore.getState().setEditingRegion("header");
                }}
              />
            )}
            {footerEditor && (
              <TextBoxOverlay
                editor={footerEditor}
                selected={selectedFooterTextBox}
                containerRef={editorRegionRef}
                onBeforeEnterTextEdit={() => {
                  // W — idem ao header: garante que o footerEditor esteja
                  // editable antes de setar a TextSelection na TextBox.
                  useLaudoStore.getState().setEditingRegion("footer");
                }}
              />
            )}
            {/* Overhaul tabelas (F2/F3) — overlay com toolbar + handles +
                menu de contexto pra tabela com cursor. Corpo + cabeçalho +
                rodapé (3 instâncias), como o TextBoxOverlay. */}
            <TableOverlay
              editor={editor}
              selected={selectedTable}
              containerRef={editorRegionRef}
              onOpenProperties={() => setTablePropsEditor(editor)}
            />
            {headerEditor && (
              <TableOverlay
                editor={headerEditor}
                selected={selectedHeaderTable}
                containerRef={editorRegionRef}
                onOpenProperties={() => setTablePropsEditor(headerEditor)}
              />
            )}
            {footerEditor && (
              <TableOverlay
                editor={footerEditor}
                selected={selectedFooterTable}
                containerRef={editorRegionRef}
                onOpenProperties={() => setTablePropsEditor(footerEditor)}
              />
            )}
            {/* F4 — Diálogo de propriedades da tabela (bordas/padding/
                largura/header). Aplica no editor que o abriu. */}
            <TablePropertiesDialog
              open={tablePropsEditor !== null}
              editor={tablePropsEditor}
              onClose={() => setTablePropsEditor(null)}
            />
          </div>
          {showInspector && (
            <Inspector
              doc={docForInspector}
              editor={editor}
              workspacePath={workspacePath}
              laudoId={currentLaudo.id}
            />
          )}
        </div>
      </LaudoErrorBoundary>

      {showStatusBar && (
        <LaudoStatusBar
          editor={editor}
          saveState={saveState}
          saveLabel={saveLabel}
          mode={mode}
          onOpenShortcuts={() => setShortcutsOpen(true)}
          pageControls={
            <PageControls
              mode={mode}
              onModeChange={setMode}
              zoom={zoom}
              onZoomChange={setZoom}
              onZoomIn={zoomIn}
              onZoomOut={zoomOut}
              onReset={resetZoom}
              onFitWidth={handleFitWidth}
              onFitPage={handleFitPage}
              orientation={
                currentDoc?.layout?.orientation === "landscape"
                  ? "landscape"
                  : "portrait"
              }
              onOrientationChange={(next) =>
                void handleOrientationChange(next)
              }
            />
          }
        />
      )}
    </div>
  );
}

/** O — Parse defensivo do EXIF JSON (que vem como string do backend).
 *  Retorna `null` em qualquer erro de parse — preferimos perder o EXIF
 *  do que crashar o editor por causa de um arquivo com header EXIF
 *  malformado. */
function safeJsonParse(s: string | null): unknown {
  if (!s) return null;
  try {
    return JSON.parse(s);
  } catch {
    return null;
  }
}
