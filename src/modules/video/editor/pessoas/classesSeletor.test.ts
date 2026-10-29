import { describe, expect, it } from "vitest";
import type { ClasseInfo, ResultadoMancha } from "@domain/video_pessoas";
import { idsDosGrupos, resumoClasses } from "./ClassesSeletor";
import { textoEtiqueta, GLIFOS } from "./fonteEtiqueta";
import { avisoDaMancha } from "./pessoasStore";

const cat: ClasseInfo[] = [
  { id: 0, nome: "pessoa", grupo: "Pessoas" },
  { id: 2, nome: "carro", grupo: "Veículos" },
  { id: 5, nome: "ônibus", grupo: "Veículos" },
  { id: 24, nome: "mochila", grupo: "Objetos de mão" },
];

describe("seletor de classes", () => {
  it("resume a escolha", () => {
    expect(resumoClasses(cat, [0])).toBe("pessoa");
    expect(resumoClasses(cat, [0, 2, 5])).toBe("Pessoas e veículos");
    expect(resumoClasses(cat, [0, 2, 5, 24])).toBe("Tudo (80)");
    expect(resumoClasses(cat, [0, 24])).toBe("2 classes");
  });
  it("acha os ids de um grupo", () => {
    expect(idsDosGrupos(cat, ["Veículos"])).toEqual([2, 5]);
    expect(idsDosGrupos(cat, ["Pessoas", "Veículos"])).toEqual([0, 2, 5]);
  });
});

describe("etiqueta de classe", () => {
  it("tira acento e põe em maiúscula, como no vídeo gravado", () => {
    expect(textoEtiqueta("ônibus")).toBe("ONIBUS");
    expect(textoEtiqueta("caminhão")).toBe("CAMINHAO");
    expect(textoEtiqueta("guarda-chuva")).toBe("GUARDA-CHUVA");
  });
  it("tem glifo para tudo que os nomes usam", () => {
    for (const c of ["ONIBUS", "CAMINHAO", "GUARDA-CHUVA", "CACHORRO-QUENTE", "MICRO-ONDAS"]) {
      for (const ch of c) expect(GLIFOS[ch], ch).toBeDefined();
    }
  });
});

describe("aviso da mancha", () => {
  it("diz onde cada sentido parou", () => {
    const r = {
      avisos: [
        { sentido: "frente", motivo: "confianca", parou_em: 75.5, quadros: 12 },
        { sentido: "tras", motivo: "limite", parou_em: null, quadros: 60 },
      ],
    } as unknown as ResultadoMancha;
    expect(avisoDaMancha(r)).toBe(
      "Para a frente: 12 quadros, perdeu a mancha em 1:15.50. Para trás: 60 quadros, chegou ao limite de quadros.",
    );
  });
});
