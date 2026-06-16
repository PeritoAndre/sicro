/**
 * W20 (S3) — Painel de Camadas estilo Photoshop para o editor de Imagem.
 *
 * - Lista todas as camadas (base, anotações, e as **camadas de pixels** criadas
 *   a partir de seleções), com a do topo da pilha em cima (igual ao Photoshop).
 * - **Reordenar arrastando** (drag & drop) ou pelas setas ↑/↓ — muda a ordem de
 *   composição das camadas de pixels.
 * - **Selecionar** uma camada a destaca (no painel e no canvas, via handles).
 * - Cabeçalho com controles da camada selecionada: **nome** (duplo-clique para
 *   renomear), **opacidade** (slider) e **trava** (lock).
 * - Por linha: miniatura, visibilidade (olho), selo de origem
 *   (original/resultado) e excluir.
 *
 * É puramente apresentacional: todas as mutações sobem por callbacks para o
 * `ImageEditor`, que é dono do `doc`.
 */
import { useState } from "react";
import {
  ChevronDown,
  ChevronUp,
  Eye,
  EyeOff,
  GripVertical,
  Lock,
  Trash2,
  Unlock,
} from "lucide-react";
import type { SicroImageLayer } from "../engine/schema";

interface Props {
  layers: SicroImageLayer[];
  pixelImages: Record<string, HTMLImageElement>;
  selectedLayerId: string | null;
  onSelect: (id: string | null) => void;
  onToggleVisible: (id: string) => void;
  onToggleLock: (id: string) => void;
  onSetOpacity: (id: string, opacity: number) => void;
  onRename: (id: string, name: string) => void;
  onDelete: (id: string) => void;
  /** Recebe a nova ordem (TOPO primeiro) — o pai mapeia para `doc.layers`. */
  onReorder: (orderedTopFirst: string[]) => void;
}

const ACCENT = "#22d3ee";
const MUTED = "rgba(148,163,184,0.85)";

export function LayersPanelPro({
  layers,
  pixelImages,
  selectedLayerId,
  onSelect,
  onToggleVisible,
  onToggleLock,
  onSetOpacity,
  onRename,
  onDelete,
  onReorder,
}: Props) {
  // Exibe o topo da pilha em cima (último do array = mais alto na composição).
  const display = [...layers].reverse();
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");

  const selected = layers.find((l) => l.id === selectedLayerId) ?? null;

  const commitReorder = (fromId: string, toId: string) => {
    if (fromId === toId) return;
    const ids = display.map((l) => l.id).filter((id) => id !== fromId);
    const idx = ids.indexOf(toId);
    if (idx < 0) return;
    ids.splice(idx, 0, fromId);
    onReorder(ids);
  };

  const moveBy = (id: string, dir: -1 | 1) => {
    const ids = display.map((l) => l.id);
    const i = ids.indexOf(id);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= ids.length) return;
    const a = ids[i];
    const b = ids[j];
    if (a === undefined || b === undefined) return;
    ids[i] = b;
    ids[j] = a;
    onReorder(ids);
  };

  const startRename = (l: SicroImageLayer) => {
    setRenamingId(l.id);
    setRenameValue(l.name);
  };
  const commitRename = () => {
    if (renamingId) {
      const v = renameValue.trim();
      if (v) onRename(renamingId, v);
    }
    setRenamingId(null);
  };

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        height: "100%",
        minHeight: 0,
      }}
    >
      {/* Controles da camada selecionada (cabeçalho estilo Photoshop). */}
      {selected ? (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 6,
            padding: "8px 10px",
            background: "rgba(2,6,12,0.4)",
            border: "1px solid rgba(148,163,184,0.18)",
            borderRadius: 6,
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              fontSize: 11,
              color: MUTED,
            }}
          >
            <span>Opacidade</span>
            <input
              type="range"
              min={0}
              max={100}
              value={Math.round((selected.opacity ?? 1) * 100)}
              onChange={(e) =>
                onSetOpacity(selected.id, Number(e.target.value) / 100)
              }
              style={{ flex: 1, accentColor: ACCENT }}
            />
            <span
              style={{
                width: 34,
                textAlign: "right",
                fontVariantNumeric: "tabular-nums",
                color: "rgba(226,232,240,0.9)",
              }}
            >
              {Math.round((selected.opacity ?? 1) * 100)}%
            </span>
          </div>
          <button
            type="button"
            onClick={() => onToggleLock(selected.id)}
            style={{
              alignSelf: "flex-start",
              display: "flex",
              alignItems: "center",
              gap: 5,
              padding: "3px 8px",
              fontSize: 10.5,
              fontFamily: "inherit",
              color: selected.locked ? "#f59e0b" : MUTED,
              background: "rgba(30,41,59,0.7)",
              border: `1px solid ${
                selected.locked ? "rgba(245,158,11,0.5)" : "rgba(148,163,184,0.25)"
              }`,
              borderRadius: 4,
              cursor: "pointer",
            }}
          >
            {selected.locked ? <Lock size={11} /> : <Unlock size={11} />}
            {selected.locked ? "Travada" : "Destravada"}
          </button>
        </div>
      ) : (
        <p style={{ margin: 0, fontSize: 11, color: MUTED, padding: "0 2px" }}>
          Selecione uma camada para ajustar opacidade, trava e ordem. Crie
          camadas a partir de uma seleção (botão “Nova camada da seleção”).
        </p>
      )}

      {/* Lista de camadas (topo da pilha em cima). */}
      <ul
        style={{
          listStyle: "none",
          margin: 0,
          padding: 0,
          display: "flex",
          flexDirection: "column",
          gap: 3,
          overflowY: "auto",
          minHeight: 0,
          flex: 1,
        }}
      >
        {display.map((l, i) => {
          const isPixels = l.kind === "pixels";
          const img = isPixels ? pixelImages[l.id] : undefined;
          const isSel = l.id === selectedLayerId;
          const isOver = overId === l.id && dragId !== l.id;
          return (
            <li
              key={l.id}
              draggable
              onDragStart={(e) => {
                setDragId(l.id);
                e.dataTransfer.effectAllowed = "move";
              }}
              onDragOver={(e) => {
                e.preventDefault();
                if (overId !== l.id) setOverId(l.id);
              }}
              onDrop={(e) => {
                e.preventDefault();
                if (dragId) commitReorder(dragId, l.id);
                setDragId(null);
                setOverId(null);
              }}
              onDragEnd={() => {
                setDragId(null);
                setOverId(null);
              }}
              onClick={() => onSelect(l.id)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "5px 6px",
                borderRadius: 5,
                cursor: "pointer",
                background: isSel
                  ? "rgba(34,211,238,0.14)"
                  : "rgba(30,41,59,0.5)",
                border: `1px solid ${
                  isSel ? "rgba(34,211,238,0.6)" : "transparent"
                }`,
                borderTop: isOver
                  ? `2px solid ${ACCENT}`
                  : `1px solid ${isSel ? "rgba(34,211,238,0.6)" : "transparent"}`,
                opacity: dragId === l.id ? 0.45 : 1,
              }}
            >
              <GripVertical
                size={12}
                color={MUTED}
                style={{ cursor: "grab", flexShrink: 0 }}
              />
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  onToggleVisible(l.id);
                }}
                title={l.visible ? "Ocultar" : "Mostrar"}
                style={iconBtn}
              >
                {l.visible ? <Eye size={12} /> : <EyeOff size={12} />}
              </button>
              {/* Miniatura (camadas de pixels) ou bolinha de cor (sistema). */}
              {img ? (
                <img
                  src={img.src}
                  alt=""
                  style={{
                    width: 26,
                    height: 26,
                    objectFit: "contain",
                    borderRadius: 3,
                    background: "rgba(2,6,12,0.6)",
                    flexShrink: 0,
                  }}
                />
              ) : (
                <span
                  style={{
                    width: 26,
                    height: 26,
                    borderRadius: 3,
                    background: "rgba(2,6,12,0.5)",
                    flexShrink: 0,
                  }}
                />
              )}
              <div style={{ flex: 1, minWidth: 0 }}>
                {renamingId === l.id ? (
                  <input
                    autoFocus
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onBlur={commitRename}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") commitRename();
                      if (e.key === "Escape") setRenamingId(null);
                    }}
                    onClick={(e) => e.stopPropagation()}
                    style={{
                      width: "100%",
                      fontSize: 11.5,
                      fontFamily: "inherit",
                      color: "#fff",
                      background: "rgba(2,6,12,0.8)",
                      border: `1px solid ${ACCENT}`,
                      borderRadius: 3,
                      padding: "1px 4px",
                    }}
                  />
                ) : (
                  <div
                    onDoubleClick={(e) => {
                      e.stopPropagation();
                      startRename(l);
                    }}
                    style={{
                      fontSize: 11.5,
                      color: "rgba(226,232,240,0.95)",
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}
                    title={`${l.name} — duplo-clique para renomear`}
                  >
                    {l.name}
                  </div>
                )}
                <div style={{ fontSize: 9.5, color: MUTED }}>
                  {isPixels
                    ? l.pixel_source === "processed"
                      ? "pixels · resultado"
                      : "pixels · original"
                    : l.kind}
                </div>
              </div>
              {/* Reordenar por setas (acessível, além do arrasto). */}
              <div style={{ display: "flex", flexDirection: "column" }}>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    moveBy(l.id, -1);
                  }}
                  disabled={i === 0}
                  title="Subir na pilha"
                  style={{ ...iconBtn, opacity: i === 0 ? 0.3 : 1, height: 13 }}
                >
                  <ChevronUp size={11} />
                </button>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    moveBy(l.id, 1);
                  }}
                  disabled={i === display.length - 1}
                  title="Descer na pilha"
                  style={{
                    ...iconBtn,
                    opacity: i === display.length - 1 ? 0.3 : 1,
                    height: 13,
                  }}
                >
                  <ChevronDown size={11} />
                </button>
              </div>
              {isPixels && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDelete(l.id);
                  }}
                  title="Excluir camada"
                  style={{ ...iconBtn, color: "rgba(248,113,113,0.85)" }}
                >
                  <Trash2 size={12} />
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

const iconBtn: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  padding: 2,
  background: "transparent",
  border: "none",
  color: "rgba(203,213,225,0.8)",
  cursor: "pointer",
  flexShrink: 0,
};
