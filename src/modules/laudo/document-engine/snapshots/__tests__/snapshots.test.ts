/**
 * Tests do subsistema de snapshots (F8).
 */

import { describe, expect, it } from "vitest";
import type { JSONContent } from "@tiptap/core";
import {
  createSnapshot,
  deleteSnapshot,
  MAX_SNAPSHOTS,
  pushSnapshot,
} from "../service";

function simpleDoc(text = "lorem ipsum dolor sit amet"): JSONContent {
  return {
    type: "doc",
    content: [
      {
        type: "paragraph",
        content: [{ type: "text", text }],
      },
    ],
  };
}

describe("createSnapshot", () => {
  it("gera id, author, created_at, content, stats", () => {
    const s = createSnapshot({
      author: "X",
      label: "test",
      content: simpleDoc(),
    });
    expect(s.id).toMatch(/.+/);
    expect(s.author).toBe("X");
    expect(s.label).toBe("test");
    expect(Date.parse(s.created_at)).not.toBeNaN();
    expect(s.content.type).toBe("doc");
    expect(s.stats?.words).toBeGreaterThan(0);
    expect(s.stats?.paragraphs).toBe(1);
  });

  it("conta palavras corretamente", () => {
    const s = createSnapshot({
      author: "X",
      content: simpleDoc("um dois tres quatro cinco"),
    });
    expect(s.stats?.words).toBe(5);
  });

  it("deep-clona o conteúdo (mutação posterior não afeta snapshot)", () => {
    const doc = simpleDoc("um");
    const s = createSnapshot({ author: "X", content: doc });
    (doc.content![0]!.content![0] as unknown as Record<string, unknown>)["text"] = "dois";
    const snapText =
      (s.content.content![0]!.content![0] as unknown as Record<string, unknown>)[
        "text"
      ];
    expect(snapText).toBe("um");
  });
});

describe("pushSnapshot", () => {
  it("insere no topo da lista", () => {
    const s1 = createSnapshot({ author: "A", content: simpleDoc("a") });
    const s2 = createSnapshot({ author: "B", content: simpleDoc("b") });
    const list = pushSnapshot(pushSnapshot([], s1), s2);
    expect(list[0]!.id).toBe(s2.id);
    expect(list[1]!.id).toBe(s1.id);
  });

  it("respeita o limite máximo (FIFO)", () => {
    let list: ReturnType<typeof createSnapshot>[] = [];
    for (let i = 0; i < MAX_SNAPSHOTS + 5; i++) {
      list = pushSnapshot(
        list,
        createSnapshot({ author: "X", content: simpleDoc(`v${i}`) }),
      );
    }
    expect(list).toHaveLength(MAX_SNAPSHOTS);
  });

  it("aceita override de maxSize", () => {
    let list: ReturnType<typeof createSnapshot>[] = [];
    for (let i = 0; i < 10; i++) {
      list = pushSnapshot(
        list,
        createSnapshot({ author: "X", content: simpleDoc(`v${i}`) }),
        5,
      );
    }
    expect(list).toHaveLength(5);
  });
});

describe("deleteSnapshot", () => {
  it("remove pelo id", () => {
    const s1 = createSnapshot({ author: "A", content: simpleDoc("a") });
    const s2 = createSnapshot({ author: "B", content: simpleDoc("b") });
    const next = deleteSnapshot([s1, s2], s1.id);
    expect(next).toHaveLength(1);
    expect(next[0]!.id).toBe(s2.id);
  });
});
