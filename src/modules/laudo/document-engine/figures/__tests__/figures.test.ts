/**
 * Testes do subsistema de figuras (F6).
 *
 * Cobre:
 *   - extractFigures: extrai figure, croqui, video_frame, photoPlate;
 *   - posições monotônicas;
 *   - photoPlate explode cada slot em uma entrada;
 *   - buildFigureList: numeração por kind vs unified;
 *   - photoPlateSlots / photoPlateColumns: tabela correta.
 */

import { describe, expect, it } from "vitest";
import type { JSONContent } from "@tiptap/core";
import { buildFigureList, extractFigures } from "../index";
import {
  photoPlateColumns,
  photoPlateSlots,
} from "../../nodes/PhotoPlate";

function doc(...children: JSONContent[]): JSONContent {
  return { type: "doc", content: children };
}
function figure(kind: string, caption: string): JSONContent {
  return {
    type: "figure",
    attrs: { kind },
    content: [
      {
        type: "figcaption",
        content: [{ type: "text", text: caption }],
      },
    ],
  };
}
function photoPlate(layout: string, photos: Array<{ caption?: string }>) {
  return {
    type: "photoPlate",
    attrs: { layout, photos },
  } as JSONContent;
}
function paragraph(text: string): JSONContent {
  return { type: "paragraph", content: [{ type: "text", text }] };
}

describe("extractFigures", () => {
  it("documento vazio → lista vazia", () => {
    expect(extractFigures(doc())).toEqual([]);
  });

  it("encontra figures por kind (image/croqui/video_frame)", () => {
    const out = extractFigures(
      doc(
        figure("image", "Vista geral"),
        figure("croqui", "Croqui esquemático"),
        figure("video_frame", "Frame do drone"),
      ),
    );
    expect(out).toHaveLength(3);
    expect(out[0]!.kind).toBe("image");
    expect(out[1]!.kind).toBe("croqui");
    expect(out[2]!.kind).toBe("video_frame");
  });

  it("captura legenda concatenada", () => {
    const out = extractFigures(doc(figure("image", "Vista geral do local")));
    expect(out[0]!.caption).toBe("Vista geral do local");
  });

  it("ignora parágrafos comuns", () => {
    const out = extractFigures(
      doc(figure("image", "F1"), paragraph("texto"), figure("image", "F2")),
    );
    expect(out).toHaveLength(2);
  });

  it("photoPlate explode em uma entrada por foto", () => {
    const out = extractFigures(
      doc(
        photoPlate("2x2", [
          { caption: "Foto 1" },
          { caption: "Foto 2" },
          { caption: "Foto 3" },
        ]),
      ),
    );
    expect(out).toHaveLength(3);
    for (const e of out) {
      expect(e.kind).toBe("photoplate");
    }
    expect(out[0]!.cellIndex).toBe(0);
    expect(out[2]!.cellIndex).toBe(2);
  });

  it("posições são monotônicas crescentes", () => {
    const out = extractFigures(
      doc(figure("image", "A"), figure("image", "B"), figure("image", "C")),
    );
    expect(out[1]!.pos).toBeGreaterThan(out[0]!.pos);
    expect(out[2]!.pos).toBeGreaterThan(out[1]!.pos);
  });

  it("kind desconhecido → 'image' como fallback", () => {
    const out = extractFigures(doc(figure("xpto" as never, "X")));
    expect(out[0]!.kind).toBe("image");
  });
});

describe("buildFigureList — modo by-kind (default)", () => {
  it("conta por bucket", () => {
    const raw = extractFigures(
      doc(
        figure("image", "F1"),
        figure("croqui", "C1"),
        figure("image", "F2"),
        figure("croqui", "C2"),
        figure("video_frame", "V1"),
      ),
    );
    const list = buildFigureList(raw);
    expect(list[0]!.label).toBe("Figura 1");
    expect(list[1]!.label).toBe("Croqui 1");
    expect(list[2]!.label).toBe("Figura 2");
    expect(list[3]!.label).toBe("Croqui 2");
    expect(list[4]!.label).toBe("Frame 1");
  });

  it("photoplate usa prefixo 'Figura'", () => {
    const raw = extractFigures(
      doc(photoPlate("2x2", [{ caption: "A" }, { caption: "B" }])),
    );
    const list = buildFigureList(raw);
    expect(list[0]!.label).toBe("Figura 1");
    expect(list[1]!.label).toBe("Figura 2");
  });
});

describe("buildFigureList — modo unified", () => {
  it("todos contam como Figura N sequencial", () => {
    const raw = extractFigures(
      doc(
        figure("image", "F1"),
        figure("croqui", "C1"),
        figure("video_frame", "V1"),
      ),
    );
    const list = buildFigureList(raw, "unified");
    expect(list.map((l) => l.label)).toEqual([
      "Figura 1",
      "Figura 2",
      "Figura 3",
    ]);
  });
});

describe("photoPlateSlots", () => {
  it("retorna número de fotos por layout", () => {
    expect(photoPlateSlots("1x1")).toBe(1);
    expect(photoPlateSlots("1x2")).toBe(2);
    expect(photoPlateSlots("2x2")).toBe(4);
    expect(photoPlateSlots("2x3")).toBe(6);
  });
});

describe("photoPlateColumns", () => {
  it("retorna número de colunas CSS do grid", () => {
    expect(photoPlateColumns("1x1")).toBe(1);
    expect(photoPlateColumns("1x2")).toBe(1);
    expect(photoPlateColumns("2x2")).toBe(2);
    expect(photoPlateColumns("2x3")).toBe(2);
  });
});
