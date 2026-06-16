/**
 * FiguresPanel — popover "Figuras" no menu superior.
 *
 * F6 — Funções:
 *
 *   1. Lista todas as figuras/croquis/storyboard do documento com
 *      numeração automática ("Figura 1", "Croqui 2", "Frame 1").
 *   2. Click em uma entrada → navega o cursor até aquela figura.
 *   3. Botão "Inserir prancha fotográfica" → abre seletor de layout
 *      (1/2/4/6) e chama `editor.insertPhotoPlate`.
 *   4. Botão "Trocar imagem da figura atual" — placeholder (F6.1).
 *
 * Reage ao `editor.on("update")` para se manter sincronizado.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import type { Editor } from "@tiptap/react";
import {
  Image as ImageIcon,
  LayoutGrid,
  Camera,
  Layers,
  CornerDownRight,
  Upload,
} from "lucide-react";
import {
  buildFigureList,
  extractFigures,
  type NumberedFigureEntry,
  type PhotoPlateLayout,
} from "../document-engine";
import styles from "./FiguresPanel.module.css";

interface FiguresPanelProps {
  editor: Editor | null;
}

export function FiguresPanel({ editor }: FiguresPanelProps) {
  const [figures, setFigures] = useState<NumberedFigureEntry[]>([]);
  const [layoutChoice, setLayoutChoice] = useState<PhotoPlateLayout>("2x2");
  const [feedback, setFeedback] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // Sincroniza com o documento sempre que muda.
  useEffect(() => {
    if (!editor) {
      setFigures([]);
      return undefined;
    }
    const refresh = () => {
      const raw = extractFigures(editor.getJSON());
      setFigures(buildFigureList(raw));
    };
    refresh();
    editor.on("update", refresh);
    return () => {
      editor.off("update", refresh);
    };
  }, [editor]);

  const counts = useMemo(() => {
    let figuras = 0;
    let croquis = 0;
    let frames = 0;
    let photoplate = 0;
    for (const f of figures) {
      if (f.kind === "croqui") croquis++;
      else if (f.kind === "video_frame") frames++;
      else if (f.kind === "photoplate") photoplate++;
      else figuras++;
    }
    return { figuras, croquis, frames, photoplate };
  }, [figures]);

  const handleJump = (pos: number) => {
    if (!editor) return;
    editor.commands.focus();
    editor.commands.setTextSelection(pos + 1);
    editor.commands.scrollIntoView();
  };

  const handleInsertPlate = () => {
    if (!editor) return;
    editor
      .chain()
      .focus()
      .insertPhotoPlate({
        layout: layoutChoice,
        title: "",
        photos: [],
      })
      .run();
  };

  // F6.1 (bug fix) — Inserir imagem externa do disco como Figure.
  // Lê o arquivo como data URL (base64) e injeta no editor. A persistência
  // é inline no `.sicrodoc` — para imagens grandes considere usar a Central
  // de Evidências (drag-and-drop) que copia para o workspace.
  const handlePickFile = () => {
    fileInputRef.current?.click();
  };

  const handleFileSelected = (event: React.ChangeEvent<HTMLInputElement>) => {
    if (!editor) return;
    const files = event.target.files;
    if (!files || files.length === 0) return;

    // Múltiplos arquivos: insere cada um sequencialmente.
    const fileList = Array.from(files);
    let inserted = 0;
    const total = fileList.length;

    fileList.forEach((file) => {
      if (!file.type.startsWith("image/")) {
        setFeedback(`"${file.name}" não é uma imagem — ignorado.`);
        return;
      }
      const reader = new FileReader();
      reader.onload = () => {
        const src = typeof reader.result === "string" ? reader.result : null;
        if (!src) return;
        // Caption padrão = nome do arquivo (sem extensão).
        const captionDefault = file.name.replace(/\.[^/.]+$/, "");
        editor
          .chain()
          .focus()
          .insertFigure({
            src,
            kind: "image",
            caption: captionDefault,
            width: "70%",
            align: "center",
          })
          .run();
        inserted += 1;
        if (inserted === total) {
          setFeedback(
            total === 1
              ? `Imagem "${file.name}" inserida.`
              : `${total} imagens inseridas.`,
          );
          setTimeout(() => setFeedback(null), 2500);
        }
      };
      reader.onerror = () => {
        setFeedback(`Erro ao ler "${file.name}".`);
      };
      reader.readAsDataURL(file);
    });

    // Reset para permitir selecionar o mesmo arquivo novamente.
    event.target.value = "";
  };

  if (!editor) {
    return (
      <p className={styles.empty}>
        Abra um laudo para visualizar e inserir figuras.
      </p>
    );
  }

  return (
    <>
      <h3 className={styles.sectionTitle}>Figuras do laudo</h3>
      <p className={styles.intro}>
        Lista numerada automaticamente. Click numa entrada para ir até
        a figura no documento.
      </p>

      {/* Counts chip */}
      <div className={styles.countsRow}>
        <span className={styles.countChip}>
          <ImageIcon size={11} /> Figuras: <strong>{counts.figuras}</strong>
        </span>
        <span className={styles.countChip}>
          <Layers size={11} /> Croquis: <strong>{counts.croquis}</strong>
        </span>
        <span className={styles.countChip}>
          <Camera size={11} /> Frames: <strong>{counts.frames}</strong>
        </span>
        <span className={styles.countChip}>
          <LayoutGrid size={11} /> Pranchas: <strong>{counts.photoplate}</strong>
        </span>
      </div>

      {/* F6.1 — Inserir imagem externa do disco */}
      <div className={styles.imageInserter}>
        <button
          type="button"
          className={styles.primaryAction}
          onClick={handlePickFile}
        >
          <Upload size={14} /> Inserir imagem do arquivo…
        </button>
        <p className={styles.hint}>
          Aceita PNG, JPG, GIF, WEBP. Múltiplos arquivos OK. Caption inicial
          = nome do arquivo (edite depois clicando na legenda).
        </p>
        <input
          ref={fileInputRef}
          type="file"
          accept="image/*"
          multiple
          style={{ display: "none" }}
          onChange={handleFileSelected}
        />
        {feedback && <div className={styles.feedback}>{feedback}</div>}
      </div>

      {/* PhotoPlate inserter */}
      <div className={styles.plateInserter}>
        <div className={styles.plateInserterTitle}>
          <LayoutGrid size={12} /> Inserir prancha fotográfica
        </div>
        <div className={styles.layoutGroup} role="radiogroup">
          <LayoutBtn
            value="1x1"
            current={layoutChoice}
            onSelect={setLayoutChoice}
            label="1 foto"
          />
          <LayoutBtn
            value="1x2"
            current={layoutChoice}
            onSelect={setLayoutChoice}
            label="2 fotos"
          />
          <LayoutBtn
            value="2x2"
            current={layoutChoice}
            onSelect={setLayoutChoice}
            label="4 fotos"
          />
          <LayoutBtn
            value="2x3"
            current={layoutChoice}
            onSelect={setLayoutChoice}
            label="6 fotos"
          />
        </div>
        <button
          type="button"
          className={styles.insertBtn}
          onClick={handleInsertPlate}
        >
          Inserir prancha {layoutChoice} no cursor
        </button>
      </div>

      {/* Lista */}
      {figures.length === 0 ? (
        <p className={styles.empty}>
          Nenhuma figura no laudo ainda. Use a aba "Evidências" do Inspector
          ou insira uma prancha acima.
        </p>
      ) : (
        <div className={styles.list}>
          {figures.map((f, idx) => (
            <button
              key={`${f.pos}-${f.kind}-${idx}`}
              type="button"
              className={`${styles.item} ${styles[`item_${f.kind}`] ?? ""}`}
              onClick={() => handleJump(f.pos)}
            >
              <div className={styles.itemHeader}>
                <span className={styles.itemLabel}>{f.label}</span>
                {f.cellIndex !== undefined && (
                  <span className={styles.itemCell}>
                    cela {f.cellIndex + 1}
                  </span>
                )}
              </div>
              <div className={styles.itemCaption}>
                <CornerDownRight size={11} className={styles.itemCaptionIcon} />
                {f.caption ? (
                  <span>{f.caption}</span>
                ) : (
                  <span className={styles.itemEmpty}>(sem legenda)</span>
                )}
              </div>
            </button>
          ))}
        </div>
      )}
    </>
  );
}

function LayoutBtn({
  value,
  current,
  onSelect,
  label,
}: {
  value: PhotoPlateLayout;
  current: PhotoPlateLayout;
  onSelect: (v: PhotoPlateLayout) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={current === value}
      className={`${styles.layoutBtn} ${current === value ? styles.layoutBtnActive : ""}`}
      onClick={() => onSelect(value)}
    >
      <span className={styles.layoutBtnIcon} data-layout={value} aria-hidden />
      <span className={styles.layoutBtnLabel}>{label}</span>
    </button>
  );
}
