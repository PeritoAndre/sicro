import { describe, expect, it } from "vitest";
import { escolherNotas, resumoNotas, semDownloads, versaoCurta } from "./notasVersao";

const chaves = ["../docs/releases/v5.2.0.md", "../docs/releases/v5.3.0.md", "../docs/releases/v4.1.3.md"];

describe("escolherNotas", () => {
  it("acha a versão exata", () => {
    expect(escolherNotas(chaves, "5.3.0")?.chave).toMatch(/v5\.3\.0\.md$/);
  });
  it("sem a exata, cai na mais próxima abaixo", () => {
    expect(escolherNotas(chaves, "5.3.2")?.versao).toBe("5.3.0");
    expect(escolherNotas(chaves, "5.2.5")?.versao).toBe("5.2.0");
  });
  it("nada abaixo: null", () => {
    expect(escolherNotas(chaves, "3.0.0")).toBeNull();
  });
});

describe("notas", () => {
  const md = "## SICRO 5.3\n\nLeitura **de imagem** no vídeo.\n\n### Downloads\n\n| a | b |\n\n### O que mudou\n\n- Modo Realce\n";
  it("tira só a seção Downloads", () => {
    const t = semDownloads(md);
    expect(t).not.toContain("| a |");
    expect(t).toContain("O que mudou");
  });
  it("resumo é a primeira frase útil, sem título nem markdown", () => {
    expect(resumoNotas(md)).toBe("Leitura de imagem no vídeo.");
  });
  it("resumo longo é cortado", () => {
    expect(resumoNotas("x".repeat(300), 20)).toHaveLength(20);
  });
  it("versão curta", () => {
    expect(versaoCurta("5.3.0")).toBe("5.3");
  });
});
