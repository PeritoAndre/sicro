/**
 * Build and unwrap `.sicrodoc` envelopes from/to TipTap document JSON.
 *
 * These helpers don't perform validation — they just shape the envelope.
 * `coerceSicroDoc` in `schema.ts` is the boundary that handles malformed
 * payloads coming back from disk.
 */

import type { JSONContent } from "@tiptap/core";
import { type SicroDoc, SCHEMA_VERSION } from "./schema";

export interface BuildEnvelopeArgs {
  document_id: string;
  occurrence_id: string;
  title: string;
  template_id: string;
  created_at: string;
  /** Defaults to `created_at` when omitted. */
  updated_at?: string;
}

/**
 * Wrap a TipTap doc inside the SICRO envelope, preserving any existing
 * metadata/layout when the caller passes a previous envelope.
 */
export function buildSicroDoc(
  args: BuildEnvelopeArgs,
  content: JSONContent,
  previous?: SicroDoc,
): SicroDoc {
  return {
    schema_version: previous?.schema_version ?? SCHEMA_VERSION,
    document_id: args.document_id,
    occurrence_id: args.occurrence_id,
    type: "laudo",
    title: args.title,
    template_id: args.template_id,
    created_at: args.created_at,
    updated_at: args.updated_at ?? new Date().toISOString(),
    metadata: previous?.metadata ?? {},
    layout: previous?.layout ?? {
      page_size: "A4",
      orientation: "portrait",
    },
    content,
  };
}

/** Extract just the TipTap document from an envelope. */
export function unwrapContent(envelope: SicroDoc): JSONContent {
  return envelope.content;
}
