/**
 * Storyboard — pericial block reproducing the canonical pattern from doc 04 §17:
 *   "imagem à esquerda, tempo/frame/descrição à direita".
 *
 * Structure:
 *   storyboard (block)
 *     └── storyboardItem (1+) — each item carries:
 *           - attrs: { src, timestamp, frame_label }
 *           - content: description (paragraph-like inline)
 *
 * In Spike B the storyboard does NOT bind to a real video evidence — the
 * `video_id` and `media_hash` from doc 04 are reserved for later spikes.
 */

import { Node, mergeAttributes } from "@tiptap/core";

export interface StoryboardEvidenceItem {
  src: string | null;
  timestamp: string;
  frame_label: string;
  description?: string;
  /** Evidence provenance (MVP 4). */
  storyboard_frame_id?: string;
  event_id?: string;
  media_hash?: string;
  pts?: number | null;
  time_base?: string | null;
  relative_path?: string | null;
}

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    storyboard: {
      insertStoryboard: (initialItems?: number) => ReturnType;
      appendStoryboardItem: () => ReturnType;
      /** MVP 4: insert a fully-populated storyboard from real video frames. */
      insertStoryboardFromVideo: (params: {
        caption?: string;
        media_hash?: string;
        items: StoryboardEvidenceItem[];
      }) => ReturnType;
    };
  }
}

export const Storyboard = Node.create({
  name: "storyboard",
  group: "block",
  content: "storyboardItem+",
  isolating: true,
  defining: true,

  addAttributes() {
    return {
      caption: { default: "Sequência observada no vídeo." },
      // MVP 4: vínculo com o vídeo de origem.
      media_hash: { default: null },
    };
  },

  parseHTML() {
    return [{ tag: "section[data-sicro-storyboard]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const extra: Record<string, string> = { "data-sicro-storyboard": "true" };
    if (node.attrs.media_hash)
      extra["data-media-hash"] = String(node.attrs.media_hash);
    return ["section", mergeAttributes(HTMLAttributes, extra), 0];
  },

  addCommands() {
    return {
      insertStoryboard:
        (initialItems = 2) =>
        ({ commands }) => {
          const items = Array.from({ length: Math.max(1, initialItems) }, (_, i) =>
            defaultStoryboardItem(i + 1),
          );
          return commands.insertContent({
            type: this.name,
            attrs: { caption: "Sequência observada no vídeo.", media_hash: null },
            content: items,
          });
        },
      appendStoryboardItem:
        () =>
        ({ chain, state }) => {
          const { $from } = state.selection;
          let depth = $from.depth;
          while (depth > 0 && $from.node(depth).type.name !== "storyboard") {
            depth -= 1;
          }
          if (depth === 0) return false;
          const sbNode = $from.node(depth);
          const insertAt = $from.before(depth) + sbNode.nodeSize - 1;
          return chain()
            .insertContentAt(insertAt, defaultStoryboardItem(sbNode.childCount + 1))
            .run();
        },
      insertStoryboardFromVideo:
        ({ caption, media_hash, items }) =>
        ({ commands }) => {
          if (items.length === 0) return false;
          const tiptapItems = items.map((it) => ({
            type: "storyboardItem",
            attrs: {
              src: it.src ?? null,
              timestamp: it.timestamp,
              frame_label: it.frame_label,
              storyboard_frame_id: it.storyboard_frame_id ?? null,
              event_id: it.event_id ?? null,
              media_hash: it.media_hash ?? media_hash ?? null,
              pts: it.pts ?? null,
              time_base: it.time_base ?? null,
              relative_path: it.relative_path ?? null,
            },
            content: [
              {
                type: "paragraph",
                content: [
                  {
                    type: "text",
                    text: it.description?.trim() || "Descrição do frame.",
                  },
                ],
              },
            ],
          }));
          return commands.insertContent({
            type: this.name,
            attrs: {
              caption: caption ?? "Sequência observada no vídeo.",
              media_hash: media_hash ?? null,
            },
            content: tiptapItems,
          });
        },
    };
  },
});

export const StoryboardItem = Node.create({
  name: "storyboardItem",
  content: "paragraph+",
  defining: true,
  isolating: true,

  addAttributes() {
    return {
      src: { default: null },
      timestamp: { default: "00:00:00.000" },
      frame_label: { default: "Frame: 0" },
      // MVP 4 — evidence provenance per item.
      storyboard_frame_id: { default: null },
      event_id: { default: null },
      media_hash: { default: null },
      pts: { default: null },
      time_base: { default: null },
      relative_path: { default: null },
    };
  },

  parseHTML() {
    return [{ tag: "article[data-sicro-storyboard-item]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const extra: Record<string, string> = {
      "data-sicro-storyboard-item": "true",
      "data-timestamp": node.attrs.timestamp ?? "",
      "data-frame": node.attrs.frame_label ?? "",
    };
    if (node.attrs.storyboard_frame_id)
      extra["data-storyboard-frame-id"] = String(node.attrs.storyboard_frame_id);
    if (node.attrs.event_id)
      extra["data-event-id"] = String(node.attrs.event_id);
    if (node.attrs.media_hash)
      extra["data-media-hash"] = String(node.attrs.media_hash);
    if (node.attrs.relative_path)
      extra["data-relative-path"] = String(node.attrs.relative_path);
    return [
      "article",
      mergeAttributes(HTMLAttributes, extra),
      [
        "div",
        { "data-sicro-storyboard-image": "true" },
        ["img", { src: node.attrs.src ?? PLACEHOLDER_FRAME, alt: "Frame placeholder" }],
      ],
      [
        "div",
        { "data-sicro-storyboard-meta": "true" },
        ["div", { "data-sicro-storyboard-time": "true" }, node.attrs.timestamp ?? ""],
        ["div", { "data-sicro-storyboard-frame": "true" }, node.attrs.frame_label ?? ""],
        ["div", { "data-sicro-storyboard-description": "true" }, 0],
      ],
    ];
  },
});

const PLACEHOLDER_FRAME =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 180">
       <rect width="100%" height="100%" fill="#1a1f2b"/>
       <text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle"
             font-family="Inter, sans-serif" font-size="14" fill="#a9b6c7">
         Frame placeholder
       </text>
     </svg>`,
  );

function defaultStoryboardItem(index: number) {
  const seconds = (index - 1) * 5;
  const mm = String(Math.floor(seconds / 60)).padStart(2, "0");
  const ss = String(seconds % 60).padStart(2, "0");
  return {
    type: "storyboardItem",
    attrs: {
      src: null,
      timestamp: `00:${mm}:${ss}.000`,
      frame_label: `Frame: ${index * 24}`,
    },
    content: [
      {
        type: "paragraph",
        content: [
          {
            type: "text",
            text: `Descrição do evento ${index}.`,
          },
        ],
      },
    ],
  };
}
