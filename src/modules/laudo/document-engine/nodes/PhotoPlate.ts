/**
 * PhotoPlate — bloco de "prancha fotográfica" pericial.
 *
 * F6 — Permite inserir múltiplas fotos em layout padronizado: 1 foto
 * por página, 2 fotos (1×2), 4 fotos (2×2) ou 6 fotos (2×3). Cada foto
 * dentro da prancha tem legenda própria + provenance opcional.
 *
 * Persistência:
 *   {
 *     type: "photoPlate",
 *     attrs: {
 *       layout: "1x1" | "1x2" | "2x2" | "2x3",
 *       title: "Prancha 1 — Local do sinistro",
 *       photos: [{ src, relative_path, caption, source, evidence_id, ... }]
 *     }
 *   }
 *
 * Decisão de design: o array de fotos vive nos `attrs` (não como
 * children TipTap). Isso simplifica:
 *   - persistência (JSON puro),
 *   - exportação DOCX/PDF (walker lê attrs uma vez, monta o grid),
 *   - edição (UI dedicada via Inspector / popover — não inline).
 *
 * Trade-off: as legendas não usam o editor TipTap; são edição via
 * formulário. Para legendas ricas (negrito, links), use o node `figure`
 * solto. O `PhotoPlate` é otimizado para o caso comum: muitas fotos
 * com legenda curta padronizada (uma frase).
 */

import { Node, mergeAttributes } from "@tiptap/core";

/** Layout suportado — controla o grid CSS. */
export type PhotoPlateLayout = "1x1" | "1x2" | "2x2" | "2x3";

export interface PhotoPlateEntry {
  /** URL ou data-URI da imagem para exibir. */
  src: string;
  /** Caminho relativo ao workspace (para portabilidade do `.sicrodoc`). */
  relative_path?: string | null;
  /** Legenda da foto individual. */
  caption?: string;
  /** Origem visual ("Foto SICRO", "Drone", "Câmera celular"). */
  source?: string;
  /** Data/hora da captura quando conhecida. */
  taken_at?: string;
  /** ID rastreável quando vem do Dossiê/Vídeo/Croqui. */
  evidence_id?: string | null;
  evidence_kind?: string | null;
  source_hash?: string | null;
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    photoPlate: {
      /** Insere uma prancha vazia ou populada. */
      insertPhotoPlate: (attrs?: {
        layout?: PhotoPlateLayout;
        title?: string;
        photos?: PhotoPlateEntry[];
      }) => ReturnType;
      /** Atualiza apenas atributos da prancha selecionada. */
      updatePhotoPlate: (attrs: {
        layout?: PhotoPlateLayout;
        title?: string;
        photos?: PhotoPlateEntry[];
      }) => ReturnType;
    };
  }
}

/** Helper público — slots por layout. */
export function photoPlateSlots(layout: PhotoPlateLayout): number {
  switch (layout) {
    case "1x1":
      return 1;
    case "1x2":
      return 2;
    case "2x2":
      return 4;
    case "2x3":
      return 6;
  }
}

/** Helper público — colunas para CSS grid-template-columns. */
export function photoPlateColumns(layout: PhotoPlateLayout): number {
  switch (layout) {
    case "1x1":
      return 1;
    case "1x2":
      return 1;
    case "2x2":
      return 2;
    case "2x3":
      return 2;
  }
}

export const PhotoPlate = Node.create({
  name: "photoPlate",
  group: "block",
  atom: true,
  draggable: true,
  selectable: true,

  addAttributes() {
    return {
      layout: { default: "2x2" as PhotoPlateLayout },
      title: { default: "" },
      photos: {
        default: [] as PhotoPlateEntry[],
        parseHTML: (el) => {
          const raw = el.getAttribute("data-photos");
          if (!raw) return [];
          try {
            const parsed = JSON.parse(raw);
            return Array.isArray(parsed) ? (parsed as PhotoPlateEntry[]) : [];
          } catch {
            return [];
          }
        },
        renderHTML: (attrs) => {
          const photos = (attrs["photos"] as PhotoPlateEntry[]) ?? [];
          return { "data-photos": JSON.stringify(photos) };
        },
      },
    };
  },

  parseHTML() {
    return [{ tag: "div[data-sicro-photoplate]" }];
  },

  /**
   * Render do PhotoPlate.
   *
   * Estrutura HTML:
   *   <div data-sicro-photoplate data-layout="2x2">
   *     <div class="sicro-photoplate-title">Prancha 1 — Local</div>
   *     <div class="sicro-photoplate-grid" style="grid-template-columns: repeat(2,1fr)">
   *       <figure class="sicro-photoplate-cell" data-cell="0">
   *         <img src="..." />
   *         <figcaption>(Foto 1) Vista geral do local.</figcaption>
   *       </figure>
   *       ...
   *     </div>
   *   </div>
   *
   * O DOCX walker / PDF / HTML renderer espelham esse layout via CSS
   * print-friendly (page-break-inside: avoid no `.sicro-photoplate-cell`).
   */
  renderHTML({ node, HTMLAttributes }) {
    const layout = (node.attrs["layout"] as PhotoPlateLayout) ?? "2x2";
    const title = (node.attrs["title"] as string) ?? "";
    const photos = ((node.attrs["photos"] as PhotoPlateEntry[]) ?? []).slice(
      0,
      photoPlateSlots(layout),
    );
    const cols = photoPlateColumns(layout);
    // Cell construído como `any[]` porque a tipagem do TipTap `DOMOutputSpec`
    // não suporta uniões complexas — usamos as `unknown[]` para escapar.
    const cells: unknown[] = [];
    photos.forEach((photo, idx) => {
      const captionText = composeCellCaption(photo, idx);
      const cellAttrs: Record<string, string> = {
        class: "sicro-photoplate-cell",
        "data-cell": String(idx),
      };
      if (photo.evidence_id) cellAttrs["data-evidence-id"] = photo.evidence_id;
      if (photo.relative_path)
        cellAttrs["data-relative-path"] = photo.relative_path;
      cells.push([
        "figure",
        cellAttrs,
        [
          "img",
          {
            src: photo.src,
            alt: photo.caption ?? `Foto ${idx + 1}`,
            style:
              "width: 100%; height: 100%; object-fit: cover; display: block;",
          },
        ],
        ["figcaption", {}, captionText],
      ]);
    });
    // Empty cells — preserva o layout durante edição.
    const totalSlots = photoPlateSlots(layout);
    const emptyCount = totalSlots - photos.length;
    for (let i = 0; i < emptyCount; i++) {
      cells.push([
        "div",
        {
          class: "sicro-photoplate-cell sicro-photoplate-cell--empty",
          "data-cell": String(photos.length + i),
          style:
            "display: flex; align-items: center; justify-content: center; min-height: 120px; background: #f1f5f9; border: 1px dashed #cbd5e1; color: #94a3b8; font-size: 11px;",
        },
        `(slot ${photos.length + i + 1} vazio)`,
      ]);
    }

    const titleBlock: unknown[] = title
      ? [["div", { class: "sicro-photoplate-title" }, title]]
      : [];

    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-sicro-photoplate": "true",
        "data-layout": layout,
        class: "sicro-photoplate",
      }),
      ...(titleBlock as never[]),
      [
        "div",
        {
          class: "sicro-photoplate-grid",
          style: `display: grid; grid-template-columns: repeat(${cols}, 1fr); gap: 8px;`,
        },
        ...(cells as never[]),
      ],
    ] as never;
  },

  addCommands() {
    return {
      insertPhotoPlate:
        (attrs) =>
        ({ commands }) => {
          return commands.insertContent({
            type: this.name,
            attrs: {
              layout: attrs?.layout ?? "2x2",
              title: attrs?.title ?? "",
              photos: attrs?.photos ?? [],
            },
          });
        },
      updatePhotoPlate:
        (attrs) =>
        ({ chain }) => {
          const patch: Record<string, unknown> = {};
          if (attrs.layout !== undefined) patch["layout"] = attrs.layout;
          if (attrs.title !== undefined) patch["title"] = attrs.title;
          if (attrs.photos !== undefined) patch["photos"] = attrs.photos;
          return chain().updateAttributes(this.name, patch).run();
        },
    };
  },
});

// ---------------------------------------------------------------------------
// Helpers internos.

function composeCellCaption(photo: PhotoPlateEntry, idx: number): string {
  const num = idx + 1;
  const parts: string[] = [`(Foto ${num})`];
  if (photo.caption) parts.push(photo.caption);
  if (photo.source) parts.push(`— ${photo.source}`);
  if (photo.taken_at) parts.push(`(${photo.taken_at})`);
  return parts.join(" ");
}
