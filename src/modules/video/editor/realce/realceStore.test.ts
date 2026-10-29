import { beforeEach, describe, expect, it } from "vitest";
import { parseRealceCopia } from "../format";
import { useRealceStore } from "./realceStore";

describe("região do realce", () => {
  beforeEach(() => useRealceStore.setState({ cantos: [], roi: null, erro: null }));

  it("dois cliques viram a região, em qualquer ordem dos cantos", () => {
    const s = useRealceStore.getState();
    s.addCanto({ x: 300.4, y: 220 });
    expect(useRealceStore.getState().roi).toBeNull();
    s.addCanto({ x: 100, y: 120.2 });
    expect(useRealceStore.getState().roi).toEqual({ x: 100, y: 120, w: 200, h: 100 });
  });

  it("região pequena demais é recusada e os cliques recomeçam", () => {
    const s = useRealceStore.getState();
    s.addCanto({ x: 10, y: 10 });
    s.addCanto({ x: 14, y: 40 });
    const st = useRealceStore.getState();
    expect(st.roi).toBeNull();
    expect(st.cantos).toHaveLength(0);
    expect(st.erro).toMatch(/pequena/);
  });
});

describe("cópias de realce", () => {
  it("reconhece a cópia estabilizada e a com lente; o resto não", () => {
    const src = { sha256: "ab", filename: "cam1.mp4" };
    expect(parseRealceCopia(JSON.stringify({ kind: "estabilizado", source: src }))?.kind).toBe("estabilizado");
    expect(parseRealceCopia(JSON.stringify({ kind: "lente_video", source: src }))?.source.filename).toBe("cam1.mp4");
    expect(parseRealceCopia(JSON.stringify({ kind: "clip", source: src }))).toBeNull();
    expect(parseRealceCopia("{ruim")).toBeNull();
    expect(parseRealceCopia(null)).toBeNull();
  });
});
