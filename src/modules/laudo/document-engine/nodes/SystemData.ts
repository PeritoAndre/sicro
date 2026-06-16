/**
 * SystemData — inline atom representing data injected by the system
 * (typically copied from the occurrence record or the dossiê).
 *
 * Carries a review state per doc 04 §30: pending → reviewed → converted.
 * Pending entries should pop in the Inspector validation panel until the
 * perito acknowledges them.
 */

import { Node, mergeAttributes } from "@tiptap/core";

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    systemData: {
      insertSystemData: (attrs: {
        source: string;
        field: string;
        value: string;
      }) => ReturnType;
    };
  }
}

export type SystemDataReviewStatus = "pending" | "reviewed" | "converted";

export const SystemData = Node.create({
  name: "systemData",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      source: { default: "occurrence" },
      field: { default: "unknown" },
      value: { default: "" },
      review_status: { default: "pending" as SystemDataReviewStatus },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-sicro-system-data]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-sicro-system-data": "true",
        "data-source": node.attrs.source ?? "",
        "data-field": node.attrs.field ?? "",
        "data-review-status": node.attrs.review_status ?? "pending",
      }),
      node.attrs.value ?? "",
    ];
  },

  addCommands() {
    return {
      insertSystemData:
        (attrs) =>
        ({ commands }) =>
          commands.insertContent({
            type: this.name,
            attrs: {
              source: attrs.source,
              field: attrs.field,
              value: attrs.value,
              review_status: "pending",
            },
          }),
    };
  },
});
