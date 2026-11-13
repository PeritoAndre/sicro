import { describe, expect, it } from "vitest";
import {
  casaBusca,
  contarEmAndamento,
  dataPorExtenso,
  estadoBackup,
  filtrarOrdenar,
  linhaMiuda,
  resumoConteudo,
  rotuloBackup,
  saudacao,
  tituloDoCaso,
} from "./casosHome";

const criado = "2026-11-11T12:00:00";

describe("tituloDoCaso", () => {
  it("laudo + nome", () => {
    expect(tituloDoCaso({ titulo: "Km 09 da Duca Serra", protocolo: "63404/26" })).toBe(
      "Laudo 63404/26 · Km 09 da Duca Serra",
    );
  });
  it("não repete o número que o nome já traz", () => {
    expect(tituloDoCaso({ titulo: "Laudo 63404/26, Km 09", protocolo: "63404/26" })).toBe("Laudo 63404/26, Km 09");
  });
  it("laudo vence a ocorrência; ocorrência sozinha", () => {
    expect(tituloDoCaso({ protocolo: "1/26", numero_bo: "118" })).toBe("Laudo 1/26");
    expect(tituloDoCaso({ numero_bo: "2026.118", titulo: "Rod. JK" })).toBe("Ocorrência 2026.118 · Rod. JK");
  });
  it("só o nome", () => {
    expect(tituloDoCaso({ titulo: "  Casa do bairro  " })).toBe("Casa do bairro");
  });
  it("sem nada: data de criação", () => {
    expect(tituloDoCaso({ created_at: criado })).toBe("Caso de 11/11/2026");
    expect(tituloDoCaso({})).toBe("Caso sem nome");
  });
});

describe("linhaMiuda", () => {
  it("data quando há título; aviso quando o título já é a data", () => {
    expect(linhaMiuda("Laudo 1/26", criado)).toBe("Caso de 11/11/2026");
    expect(linhaMiuda("Caso de 11/11/2026", criado)).toBe("sem número de laudo");
  });
});

describe("saudacao", () => {
  it("pela hora", () => {
    expect(saudacao(4)).toBe("Boa noite");
    expect(saudacao(5)).toBe("Bom dia");
    expect(saudacao(11)).toBe("Bom dia");
    expect(saudacao(12)).toBe("Boa tarde");
    expect(saudacao(17)).toBe("Boa tarde");
    expect(saudacao(18)).toBe("Boa noite");
    expect(saudacao(23)).toBe("Boa noite");
  });
  it("com o primeiro nome do perfil", () => {
    expect(saudacao(22, "André Barroso")).toBe("Boa noite, André");
    expect(saudacao(9, "   ")).toBe("Bom dia");
    expect(saudacao(9, null)).toBe("Bom dia");
  });
});

describe("dataPorExtenso", () => {
  it("dia da semana, dia e mês por extenso", () => {
    expect(dataPorExtenso(new Date(2026, 10, 13))).toBe("Sexta, 13 de novembro de 2026");
    expect(dataPorExtenso(new Date(2026, 2, 1))).toBe("Domingo, 1 de março de 2026");
  });
});

describe("filtrarOrdenar", () => {
  const casos = [
    { workspace_id: "a", status: "aberta", last_opened_at: "2026-11-01T10:00:00Z" },
    { workspace_id: "b", status: "concluida", last_opened_at: "2026-11-10T10:00:00Z" },
    { workspace_id: "c", status: "em_andamento", last_opened_at: "2026-11-05T10:00:00Z" },
    { workspace_id: "d", status: "arquivada", last_opened_at: "2026-10-01T10:00:00Z" },
  ];
  const ids = (l: typeof casos) => l.map((c) => c.workspace_id);

  it("mais recente primeiro", () => {
    expect(ids(filtrarOrdenar(casos, "todos", new Set()))).toEqual(["b", "c", "a", "d"]);
  });
  it("fixados no topo, cada grupo por recência", () => {
    expect(ids(filtrarOrdenar(casos, "todos", new Set(["a", "d"])))).toEqual(["a", "d", "b", "c"]);
  });
  it("filtros de andamento e concluídos", () => {
    expect(ids(filtrarOrdenar(casos, "andamento", new Set()))).toEqual(["c", "a"]);
    expect(ids(filtrarOrdenar(casos, "concluidos", new Set()))).toEqual(["b", "d"]);
  });
  it("não altera a lista de entrada", () => {
    const copia = [...casos];
    filtrarOrdenar(casos, "todos", new Set(["d"]));
    expect(casos).toEqual(copia);
  });
  it("conta os em andamento", () => {
    expect(contarEmAndamento(casos)).toBe(2);
  });
});

describe("casaBusca", () => {
  it("sem acento, sem caixa, todas as palavras", () => {
    expect(casaBusca(["Laudo 63404/26 · Ocorrência"], "ocorrencia 634")).toBe(true);
    expect(casaBusca(["Laudo 63404/26"], "laudo 999")).toBe(false);
    expect(casaBusca([null, "Macapá"], "MACAPA")).toBe(true);
    expect(casaBusca(["x"], "   ")).toBe(true);
  });
});

describe("resumoConteudo", () => {
  it("junta o que há, no singular e no plural", () => {
    expect(resumoConteudo({ croquis: 1, videos: 3, image_analyses: 0 })).toBe("1 croqui · 3 vídeos");
    expect(resumoConteudo({ croquis: 2, videos: 0, audios: 1, image_analyses: 4 })).toBe(
      "2 croquis · 1 áudio · 4 imagens",
    );
    expect(resumoConteudo({ croquis: 0, videos: 0, image_analyses: 0 })).toBe("vazio");
    expect(resumoConteudo(null)).toBeNull();
  });
});

describe("backup", () => {
  const agora = new Date(2026, 10, 13, 10, 0);
  it("estado pelo tempo desde o último", () => {
    expect(estadoBackup(null, agora)).toBe("nunca");
    expect(estadoBackup(new Date(2026, 10, 13, 1, 12).toISOString(), agora)).toBe("em-dia");
    expect(estadoBackup(new Date(2026, 10, 1, 1, 12).toISOString(), agora)).toBe("atrasado");
    expect(estadoBackup("lixo", agora)).toBe("nunca");
  });
  it("rótulo de dia e hora", () => {
    expect(rotuloBackup(new Date(2026, 10, 13, 1, 12).toISOString(), agora)).toBe("hoje, 01:12");
    expect(rotuloBackup(new Date(2026, 10, 12, 23, 40).toISOString(), agora)).toBe("ontem, 23:40");
    expect(rotuloBackup(new Date(2026, 10, 9, 18, 5).toISOString(), agora)).toBe("09/11/2026, 18:05");
  });
});
