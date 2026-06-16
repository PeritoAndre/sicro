/**
 * Signature — bloco de assinatura institucional do laudo.
 *
 * Atom node (não-editável internamente). Carrega cidade/UF/data/nome/cargo
 * como atributos; a UI futura terá um editor dedicado (popover sobre o
 * bloco). No MVP 2 os atributos são inseridos com defaults sensatos e
 * podem ser ajustados pela aba "Dados" do Inspector ou diretamente no JSON.
 */

import { Node, mergeAttributes } from "@tiptap/core";

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    signature: {
      insertSignature: (attrs?: {
        city?: string;
        uf?: string;
        date?: string;
        name?: string;
        role?: string;
      }) => ReturnType;
    };
  }
}

export const Signature = Node.create({
  name: "signature",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,

  addAttributes() {
    return {
      city: { default: "Macapá" },
      uf: { default: "AP" },
      // Stored as ISO-8601 date (YYYY-MM-DD); rendered in pt-BR.
      date: { default: null },
      name: { default: "" },
      role: { default: "Perito Criminal" },
    };
  },

  parseHTML() {
    return [{ tag: "section[data-sicro-signature]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const isoDate: string | null = node.attrs.date ?? null;
    const date = isoDate ? formatPtBrDate(isoDate) : "______ / ______ / ______";
    return [
      "section",
      mergeAttributes(HTMLAttributes, { "data-sicro-signature": "true" }),
      [
        "div",
        { "data-sicro-signature-place": "true" },
        `${node.attrs.city ?? ""} - ${node.attrs.uf ?? ""}, ${date}.`,
      ],
      ["div", { "data-sicro-signature-rule": "true" }, "_______________________________"],
      [
        "div",
        { "data-sicro-signature-name": "true" },
        node.attrs.name ? String(node.attrs.name) : "(nome do perito)",
      ],
      [
        "div",
        { "data-sicro-signature-role": "true" },
        node.attrs.role ?? "Perito Criminal",
      ],
    ];
  },

  addCommands() {
    return {
      insertSignature:
        (attrs) =>
        ({ commands }) => {
          const today = new Date();
          const iso = today.toISOString().slice(0, 10);
          return commands.insertContent({
            type: this.name,
            attrs: {
              city: attrs?.city ?? "Macapá",
              uf: attrs?.uf ?? "AP",
              date: attrs?.date ?? iso,
              name: attrs?.name ?? "",
              role: attrs?.role ?? "Perito Criminal",
            },
          });
        },
    };
  },
});

function formatPtBrDate(iso: string): string {
  // iso may be `YYYY-MM-DD` or a full ISO timestamp; handle both.
  const yyyymmdd = iso.length >= 10 ? iso.slice(0, 10) : iso;
  const parts = yyyymmdd.split("-");
  if (parts.length !== 3) return yyyymmdd;
  const [yy, mm, dd] = parts;
  return `${dd}/${mm}/${yy}`;
}
