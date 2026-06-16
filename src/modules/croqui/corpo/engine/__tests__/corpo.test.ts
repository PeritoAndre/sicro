import { describe, it, expect } from "vitest";
import {
  BODY_TEMPLATES,
  BODY_VIEW_ORDER,
  BODY_TEMPLATE_GROUPS,
  LEGACY_TEMPLATE_MAP,
  LESAO_TIPOS,
  lesaoMeta,
  isLesaoTipo,
  REGIOES,
  regiaoComLado,
  POP_FRENTE,
  POP_COSTAS,
  popRegiaoId,
  popRegiaoLabel,
  isPopRegiao,
  popListaLinhas,
  coerceCorpoDoc,
  nextMarkerNumber,
  makeCorpoDoc,
  makeLesao,
  buildLegend,
  summarizeLesoes,
  type SicroCorpoDoc,
} from "../index";

describe("corpo — pranchas (artes bitmap POP)", () => {
  it("tem as 11 pranchas, todas com arte, dimensões e ≥1 vista", () => {
    expect(BODY_VIEW_ORDER).toHaveLength(11);
    for (const v of BODY_VIEW_ORDER) {
      const t = BODY_TEMPLATES[v];
      expect(t.id).toBe(v);
      expect(t.label.length).toBeGreaterThan(0);
      expect(t.group.length).toBeGreaterThan(0);
      expect(t.src.length).toBeGreaterThan(0);
      expect(t.width).toBeGreaterThan(0);
      expect(t.height).toBeGreaterThan(0);
      expect(t.views.length).toBeGreaterThanOrEqual(1);
      // Boxes das vistas: dentro da prancha (recortada ou não).
      for (const view of t.views) {
        expect(view.box.w).toBeGreaterThan(0);
        expect(view.box.h).toBeGreaterThan(0);
        expect(view.box.x).toBeGreaterThanOrEqual(0);
        expect(view.box.y).toBeGreaterThanOrEqual(0);
        expect(view.box.x + view.box.w).toBeLessThanOrEqual(t.width);
        expect(view.box.y + view.box.h).toBeLessThanOrEqual(t.height);
      }
      // Pranchas com crop: dims = dims do recorte.
      if (t.crop) {
        expect(t.crop.w).toBe(t.width);
        expect(t.crop.h).toBe(t.height);
      }
    }
  });

  it("perfis masc/fem dividem a mesma arte via crop (pranchas independentes)", () => {
    const m = BODY_TEMPLATES.corpo_lateral_masc;
    const f = BODY_TEMPLATES.corpo_lateral_fem;
    expect(m.src).toBe(f.src);
    expect(m.crop).toBeTruthy();
    expect(f.crop).toBeTruthy();
    // Recortes não se sobrepõem (masc à esquerda, fem à direita).
    expect(m.crop!.x + m.crop!.w).toBeLessThanOrEqual(f.crop!.x);
  });

  it("só os corpos inteiros frente+costas são numerados (conforme o POP)", () => {
    const numerados = BODY_VIEW_ORDER.filter((v) => BODY_TEMPLATES[v].numbered);
    expect(numerados.sort()).toEqual(["corpo_fem", "corpo_masc"]);
    for (const v of numerados) {
      const ids = BODY_TEMPLATES[v].views.map((vw) => vw.id).sort();
      expect(ids).toEqual(["costas", "frente"]);
    }
  });

  it("grupos do seletor cobrem todas as pranchas, sem repetição", () => {
    const all = BODY_TEMPLATE_GROUPS.flatMap((g) => g.views);
    expect([...all].sort()).toEqual([...BODY_VIEW_ORDER].sort());
    expect(new Set(all).size).toBe(all.length);
  });

  it("mapa legado cobre as pranchas antigas", () => {
    expect(LEGACY_TEMPLATE_MAP["corpo_completo"]).toBe("corpo_masc");
    expect(LEGACY_TEMPLATE_MAP["anterior"]).toBe("corpo_masc");
    expect(LEGACY_TEMPLATE_MAP["posterior"]).toBe("corpo_masc");
    expect(LEGACY_TEMPLATE_MAP["cabeca_frontal"]).toBe("cabeca_frontal_dorsal");
    expect(LEGACY_TEMPLATE_MAP["corpo_lateral"]).toBe("corpo_lateral_masc");
  });

  it("marcações são AUTOCONTIDAS por prancha (coerce carimba + legenda filtra)", () => {
    const doc = coerceCorpoDoc({
      corpo_id: "c1",
      occurrence_id: "occ1",
      template_id: "corpo_masc",
      canvas: { width_px: 1536, height_px: 1024 },
      markers: [
        // sem campo template (doc antigo) → herda a prancha do doc
        { id: "a", number: 1, x: 10, y: 10, tipo: "faf_entrada" },
        // de outra prancha → preservada
        { id: "b", number: 2, x: 20, y: 20, tipo: "contusao", template: "mao_direita" },
      ],
    });
    expect(doc.markers[0]!.template).toBe("corpo_masc");
    expect(doc.markers[1]!.template).toBe("mao_direita");
    // Legenda filtrada pela prancha ativa.
    expect(buildLegend(doc, "corpo_masc")).toHaveLength(1);
    expect(buildLegend(doc, "mao_direita")).toHaveLength(1);
    expect(buildLegend(doc)).toHaveLength(2); // sem filtro = todas
  });
});

describe("corpo — nômina POP (Anexo 1)", () => {
  it("frente 1–28 e costas 1–21, números únicos e sequenciais", () => {
    expect(POP_FRENTE).toHaveLength(28);
    expect(POP_COSTAS).toHaveLength(21);
    expect(POP_FRENTE.map((r) => r.n)).toEqual(
      Array.from({ length: 28 }, (_, i) => i + 1),
    );
    expect(POP_COSTAS.map((r) => r.n)).toEqual(
      Array.from({ length: 21 }, (_, i) => i + 1),
    );
  });

  it("todos os pontos são normalizados (0..1) e bilaterais têm 2 pontos", () => {
    for (const r of [...POP_FRENTE, ...POP_COSTAS]) {
      expect(r.pts.length === 1 || r.pts.length === 2).toBe(true);
      for (const [nx, ny] of r.pts) {
        expect(nx).toBeGreaterThanOrEqual(0);
        expect(nx).toBeLessThanOrEqual(1);
        expect(ny).toBeGreaterThanOrEqual(0);
        expect(ny).toBeLessThanOrEqual(1);
      }
      // Bilateral: pontos espelhados em lados opostos da linha média.
      if (r.pts.length === 2) {
        const [a, b] = r.pts;
        expect(a![0]).toBeLessThan(0.5);
        expect(b![0]).toBeGreaterThan(0.5);
      }
    }
  });

  it("grafias verbatim do manual preservadas", () => {
    expect(POP_COSTAS.find((r) => r.n === 10)?.label).toBe("Espondiléia");
    expect(POP_COSTAS.find((r) => r.n === 17)?.label).toBe("Deltodiana");
    expect(POP_COSTAS.find((r) => r.n === 11)?.label).toBe("Sacro-coccígea");
    expect(POP_FRENTE.find((r) => r.n === 20)?.label).toBe("Vulvar");
  });

  it("id/label/lista helpers", () => {
    expect(popRegiaoId("frente", 11)).toBe("pop_frente_11");
    expect(popRegiaoLabel("pop_frente_11")).toBe("11. Torácicas (frente)");
    expect(popRegiaoLabel("desconhecida")).toBeNull();
    expect(isPopRegiao("pop_costas_21")).toBe(true);
    expect(isPopRegiao("toracica_ant")).toBe(false);
    expect(popListaLinhas("frente")[0]).toBe("1. Frontal");
    expect(popListaLinhas("costas")).toHaveLength(21);
  });
});

describe("corpo — taxonomia e regiões", () => {
  it("todo tipo tem cor, rótulo e abreviatura; índice consistente", () => {
    for (const m of LESAO_TIPOS) {
      expect(m.label.length).toBeGreaterThan(0);
      expect(m.short.length).toBeGreaterThan(0);
      expect(m.color).toMatch(/^#[0-9a-f]{6}$/i);
      expect(lesaoMeta(m.tipo)).toBe(m);
    }
  });

  it("isLesaoTipo valida", () => {
    expect(isLesaoTipo("faf_entrada")).toBe(true);
    expect(isLesaoTipo("xpto")).toBe(false);
    expect(isLesaoTipo(42)).toBe(false);
  });

  it("regiaoComLado combina região + lateralidade (legado e POP)", () => {
    expect(regiaoComLado("antebraco_ant", "D")).toBe("Antebraço (D)");
    expect(regiaoComLado("toracica_ant", "central")).toBe("Torácica (mamária)");
    expect(regiaoComLado("pop_frente_11", "E")).toBe(
      "11. Torácicas (frente) (E)",
    );
    expect(regiaoComLado(null, "D")).toBe("");
    expect(REGIOES.length).toBeGreaterThan(10);
  });
});

describe("corpo — factories", () => {
  it("makeCorpoDoc usa template e timestamp injetado (default corpo_masc)", () => {
    const doc = makeCorpoDoc("c1", "occ1", {
      template_id: "corpo_fem",
      title: "Vítima 1",
      now: "2026-06-06T00:00:00Z",
    });
    expect(doc.template_id).toBe("corpo_fem");
    expect(doc.title).toBe("Vítima 1");
    expect(doc.created_at).toBe("2026-06-06T00:00:00Z");
    expect(doc.canvas).toEqual({
      width_px: BODY_TEMPLATES.corpo_fem.width,
      height_px: BODY_TEMPLATES.corpo_fem.height,
    });
    expect(doc.markers).toEqual([]);

    const def = makeCorpoDoc("c2", "occ1", { now: "x" });
    expect(def.template_id).toBe("corpo_masc");
  });

  it("makeLesao cria marcador com defaults nulos", () => {
    const m = makeLesao(10, 20, "faf_entrada", 1);
    expect(m).toMatchObject({
      number: 1,
      x: 10,
      y: 20,
      tipo: "faf_entrada",
      regiao: null,
      size: 12,
    });
    expect(m.id).toMatch(/^lesao_/);
  });

  it("nextMarkerNumber = maior + 1 (determinístico, robusto a gaps)", () => {
    const doc = makeCorpoDoc("c1", "occ1", { now: "x" });
    expect(nextMarkerNumber(doc)).toBe(1);
    doc.markers.push(makeLesao(0, 0, "outro", 1));
    doc.markers.push(makeLesao(0, 0, "outro", 5)); // gap proposital
    expect(nextMarkerNumber(doc)).toBe(6);
  });
});

describe("corpo — coerceCorpoDoc", () => {
  it("lança sem corpo_id/occurrence_id", () => {
    expect(() => coerceCorpoDoc({})).toThrow();
    expect(() => coerceCorpoDoc(null)).toThrow();
  });

  it("preenche defaults e normaliza template inválido pra corpo_masc", () => {
    const doc = coerceCorpoDoc({
      corpo_id: "c1",
      occurrence_id: "occ1",
      template_id: "lado_esquerdo_inexistente",
    });
    expect(doc.template_id).toBe("corpo_masc");
    expect(doc.title).toBe("Croqui corporal");
    expect(doc.canvas.width_px).toBe(BODY_TEMPLATES.corpo_masc.width);
    expect(doc.markers).toEqual([]);
  });

  it("RETROCOMPAT: template legado mapeia + markers re-escalados pro canvas novo", () => {
    // Doc da 1ª geração: prancha SVG "corpo_completo" (1040×700) com uma
    // lesão no centro exato. Após coerce, prancha = corpo_masc (1536×1024)
    // e a lesão continua no centro (escala proporcional).
    const doc = coerceCorpoDoc({
      corpo_id: "c1",
      occurrence_id: "occ1",
      template_id: "corpo_completo",
      canvas: { width_px: 1040, height_px: 700 },
      markers: [{ id: "a", number: 1, x: 520, y: 350, tipo: "faf_entrada" }],
    });
    expect(doc.template_id).toBe("corpo_masc");
    expect(doc.canvas).toEqual({ width_px: 1536, height_px: 1024 });
    expect(doc.markers[0]!.x).toBeCloseTo(768, 0);
    expect(doc.markers[0]!.y).toBeCloseTo(512, 0);
    // Prancha de cabeça legada → cabeça frontal/dorsal nova.
    const cab = coerceCorpoDoc({
      corpo_id: "c2",
      occurrence_id: "occ1",
      template_id: "cabeca_frontal",
    });
    expect(cab.template_id).toBe("cabeca_frontal_dorsal");
  });

  it("coage marcadores: tipo inválido vira 'outro', lateralidade inválida vira null, não-objetos somem", () => {
    const doc = coerceCorpoDoc({
      corpo_id: "c1",
      occurrence_id: "occ1",
      markers: [
        { id: "a", number: 1, x: 5, y: 6, tipo: "faf_saida", lateralidade: "D" },
        { id: "b", number: 2, x: 1, y: 1, tipo: "inexistente", lateralidade: "X" },
        "lixo",
        null,
      ],
    });
    expect(doc.markers).toHaveLength(2);
    expect(doc.markers[0]).toMatchObject({ tipo: "faf_saida", lateralidade: "D" });
    expect(doc.markers[1]).toMatchObject({ tipo: "outro", lateralidade: null });
  });

  it("é idempotente (coerce(coerce(x)) === coerce(x)) — inclusive vindo de doc legado", () => {
    const once = coerceCorpoDoc({
      corpo_id: "c1",
      occurrence_id: "occ1",
      template_id: "cabeca_frontal",
      canvas: { width_px: 300, height_px: 360 },
      markers: [{ id: "a", number: 1, x: 5, y: 6, tipo: "mordida" }],
    });
    const twice = coerceCorpoDoc(once);
    expect(twice).toEqual(once);
  });
});

describe("corpo — legenda", () => {
  function docComLesoes(): SicroCorpoDoc {
    const doc = makeCorpoDoc("c1", "occ1", { now: "x" });
    doc.markers = [
      {
        ...makeLesao(10, 10, "faf_entrada", 2),
        regiao: "pop_frente_11",
        lateralidade: "E",
        instrumento: "PAF",
        dimensoes_cm: "0,9 cm",
      },
      {
        ...makeLesao(20, 20, "arma_branca", 1),
        regiao: "mesogastrica", // id LEGADO — continua resolvendo
        observacao: "borda regular",
      },
    ];
    return doc;
  }

  it("buildLegend ordena por número e resolve regiões POP e legadas", () => {
    const rows = buildLegend(docComLesoes());
    expect(rows).toHaveLength(2);
    expect(rows.map((r) => r.number)).toEqual([1, 2]);
    expect(rows[1]).toMatchObject({
      number: 2,
      tipo: "FAF — orifício de entrada",
      regiao: "11. Torácicas (frente) (E)",
      instrumento: "PAF",
      dimensoes: "0,9 cm",
    });
    expect(rows[0]!.regiao).toBe("Mesogástrica (umbilical)");
    expect(rows[0]!.color).toMatch(/^#/);
  });

  it("summarizeLesoes conta por tipo", () => {
    const doc = makeCorpoDoc("c1", "occ1", { now: "x" });
    expect(summarizeLesoes(doc)).toMatch(/nenhuma/i);
    doc.markers = [
      makeLesao(0, 0, "faf_entrada", 1),
      makeLesao(0, 0, "faf_entrada", 2),
      makeLesao(0, 0, "arma_branca", 3),
    ];
    const s = summarizeLesoes(doc);
    expect(s).toContain("2×");
    expect(s).toContain("FAF — orifício de entrada");
  });
});
