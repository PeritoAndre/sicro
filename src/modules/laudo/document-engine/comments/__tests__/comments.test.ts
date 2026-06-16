/**
 * Tests do subsistema de comentários (F8).
 */

import { describe, expect, it } from "vitest";
import type { JSONContent } from "@tiptap/core";
import {
  addComment,
  addReply,
  countActiveComments,
  createComment,
  deleteComment,
  extractCommentAnchors,
  resolveComment,
  unresolveComment,
  updateComment,
} from "../service";

describe("createComment", () => {
  it("gera id, author, body, created_at e resolved=false", () => {
    const c = createComment({ author: "Ana", body: "Texto" });
    expect(c.id).toMatch(/.+/);
    expect(c.author).toBe("Ana");
    expect(c.body).toBe("Texto");
    expect(c.resolved).toBe(false);
    expect(Date.parse(c.created_at)).not.toBeNaN();
  });
});

describe("addComment / updateComment / resolveComment / unresolveComment / deleteComment", () => {
  it("addComment cria nova lista (imutável)", () => {
    const c = createComment({ author: "X", body: "y" });
    const before: never[] = [];
    const after = addComment(before, c);
    expect(after).toHaveLength(1);
    expect(before).toHaveLength(0);
  });

  it("updateComment patcha apenas o id correto", () => {
    const c1 = createComment({ author: "X", body: "a" });
    const c2 = createComment({ author: "Y", body: "b" });
    const list = [c1, c2];
    const next = updateComment(list, c1.id, { body: "c" });
    expect(next[0]!.body).toBe("c");
    expect(next[1]!.body).toBe("b");
    expect(c1.body).toBe("a"); // imutável
  });

  it("resolveComment / unresolveComment alteram a flag", () => {
    const c = createComment({ author: "X", body: "y" });
    const r = resolveComment([c], c.id);
    expect(r[0]!.resolved).toBe(true);
    const u = unresolveComment(r, c.id);
    expect(u[0]!.resolved).toBe(false);
  });

  it("deleteComment remove pelo id", () => {
    const c1 = createComment({ author: "X", body: "a" });
    const c2 = createComment({ author: "Y", body: "b" });
    const next = deleteComment([c1, c2], c1.id);
    expect(next).toHaveLength(1);
    expect(next[0]!.id).toBe(c2.id);
  });
});

describe("addReply", () => {
  it("anexa uma resposta ao comentário correto", () => {
    const c = createComment({ author: "X", body: "y" });
    const next = addReply([c], c.id, { author: "Z", body: "ok" });
    expect(next[0]!.replies).toHaveLength(1);
    expect(next[0]!.replies![0]!.author).toBe("Z");
    expect(next[0]!.replies![0]!.body).toBe("ok");
  });
});

describe("countActiveComments", () => {
  it("ignora resolvidos", () => {
    const c1 = createComment({ author: "X", body: "a" });
    const c2 = { ...createComment({ author: "Y", body: "b" }), resolved: true };
    expect(countActiveComments([c1, c2])).toBe(1);
  });
});

describe("extractCommentAnchors", () => {
  function withCommentMark(text: string, id: string): JSONContent {
    return {
      type: "text",
      text,
      marks: [{ type: "commentMark", attrs: { id, resolved: false } }],
    };
  }

  it("retorna lista vazia para doc vazio", () => {
    expect(extractCommentAnchors(null)).toEqual([]);
    expect(extractCommentAnchors({ type: "doc", content: [] })).toEqual([]);
  });

  it("extrai um anchor único", () => {
    const doc: JSONContent = {
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [withCommentMark("trecho", "cmt-1")],
        },
      ],
    };
    const out = extractCommentAnchors(doc);
    expect(out).toHaveLength(1);
    expect(out[0]!.id).toBe("cmt-1");
    expect(out[0]!.excerpt).toBe("trecho");
  });

  it("concatena texto quando mesmo id aparece em múltiplos text-runs", () => {
    const doc: JSONContent = {
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [
            withCommentMark("Parte 1 ", "cmt-1"),
            withCommentMark("Parte 2", "cmt-1"),
          ],
        },
      ],
    };
    const out = extractCommentAnchors(doc);
    expect(out).toHaveLength(1);
    expect(out[0]!.excerpt).toContain("Parte 1");
    expect(out[0]!.excerpt).toContain("Parte 2");
  });

  it("limita excerpt a 80 chars", () => {
    const longText = "a".repeat(200);
    const doc: JSONContent = {
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [withCommentMark(longText, "cmt-1")],
        },
      ],
    };
    const out = extractCommentAnchors(doc);
    expect(out[0]!.excerpt.length).toBeLessThanOrEqual(80);
  });
});
