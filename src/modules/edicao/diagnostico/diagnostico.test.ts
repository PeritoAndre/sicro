import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { Agenda, ambienteFalso, fonteCodigo, fonteR, type FonteFalsa } from "./falsos";
import { baseDeTempo, previstoAbsoluto, tempo3 } from "./tempo3";
import { pool8 } from "./pool8";
import { compose } from "./compose";
import { trocaDeClipe } from "./switch";
import { diagnosticar, isoLocal, motorDoWebview, PLANO, type Dependencias, type Etapa, type Relatorio } from "./executar";
import { resumir } from "./resumo";
import type { MidiasDiagnostico } from "./comandos";

const cams = (suf: string, w: number, h: number) =>
  Object.fromEntries([1, 2, 3, 4].map((i) => [`cam${i}${suf}.mp4`, fonteCodigo(w, h)])) as Record<string, FonteFalsa>;

describe("tempo3", () => {
  it("calibra, lê o quadro pelos dois níveis e separa tempo absoluto de rebaseado", async () => {
    // r3.mp4 como o WebKitGTK e o Chromium (absoluto); r3.mkv como o WebKitGTK (rebaseado pelo início)
    const amb = ambienteFalso({ "r0.mp4": fonteR(0), "r3.mp4": fonteR(3), "r3.mkv": fonteR(0) });
    const r = await amb.ag.rodar(tempo3(amb, ["r0.mp4", "r3.mp4", "r3.mkv", "falta.mkv"]));
    expect(r.calib.fino).toHaveLength(16);
    expect(r.calib.grosso).toHaveLength(12);
    expect(new Set(r.calib.fino).size).toBe(16);
    const [r0, r3mp4, r3mkv, falta] = r.arquivos;
    expect(r0?.q0).toBe(0);
    // alvo 1,0167 s no r0 = quadro 30
    expect(r0?.buscas?.find((b) => b[0] === 1.0167)?.[2]).toBe(30);
    expect(baseDeTempo(r0!, 0)).toBe("absoluto");
    expect(baseDeTempo(r3mp4!, 3)).toBe("absoluto");
    expect(r3mp4?.buscas?.find((b) => b[0] === 4.0167)?.[2]).toBe(30);
    expect(baseDeTempo(r3mkv!, 3)).toBe("rebaseado");
    expect(r3mkv?.buscas?.find((b) => b[0] === 4.0167)?.[2]).toBe(120);
    expect(r3mkv?.tocar?.q_ate).toBeGreaterThan(r3mkv?.tocar?.q_de as number);
    expect(falta?.meta).toBe("timeout");
    expect(falta?.q0).toBeNull();
    expect(amb.vivos.size).toBe(0);
  });

  it("modo previsto: cópia com o p0 certo dá 0 erro; p0 de fonte rebaseada acusa", async () => {
    const amb = ambienteFalso({ "r0.mp4": fonteR(0), "cp_r3_mkv.mp4": fonteR(0.021), "r3.mkv": fonteR(0) });
    const r = await amb.ag.rodar(tempo3(amb, ["cp_r3_mkv.mp4", "r3.mkv"], [0.021, 3]));
    expect(r.arquivos[0]?.erros_previsto).toBe(0);
    expect(r.arquivos[0]?.buscas?.map((b) => b[2])).toEqual(r.arquivos[0]?.previsto);
    // só o último alvo (depois do fim, preso no último quadro) coincide
    expect(r.arquivos[1]?.erros_previsto).toBe(10);
  });

  it("previsto absoluto fora do arquivo é null", () => {
    expect(previstoAbsoluto(3.0167, 3)).toBe(0);
    expect(previstoAbsoluto(2.9, 3)).toBeNull();
    expect(previstoAbsoluto(9.1, 3)).toBeNull();
  });
});

describe("pool8", () => {
  it("prepara os outros N com os atuais tocando e lê o quadro de entrada certo", async () => {
    const amb = ambienteFalso(cams("_p720", 1280, 720));
    const r = await amb.ag.rodar(pool8(amb, { suf: "_p720", ciclos: 2, dur: 600, base_ms: 500 }));
    expect(r.ciclos).toHaveLength(2);
    for (const c of r.ciclos) {
      expect(c.seeked).toEqual(["ok", "ok", "ok", "ok"]);
      expect(c.quadro_entrada_lido).toEqual([c.quadro_entrada_esperado, c.quadro_entrada_esperado, c.quadro_entrada_esperado, c.quadro_entrada_esperado]);
      expect(c.todos_andando_ms).not.toBeNull();
    }
    expect(r.ciclos.map((c) => c.quadro_entrada_esperado)).toEqual([270, 360]);
    expect(r.base?.fps).toBeGreaterThan(50);
    expect(r.base?.fps).toBeLessThan(70);
    expect(r.quadros).toHaveLength(8);
    expect(amb.vivos.size).toBe(0);
  });

  it("n = 2 abre só cam1 e cam2", async () => {
    const amb = ambienteFalso(cams("_p720", 1280, 720));
    const r = await amb.ag.rodar(pool8(amb, { suf: "_p720", n: 2, ciclos: 1, dur: 300, base_ms: 100 }));
    expect(r.ciclos[0]?.quadro_entrada_lido).toHaveLength(2);
  });
});

describe("compose", () => {
  it("mede os sete laços e a rajada e lê o quadro no fim", async () => {
    const amb = ambienteFalso({ "v1080_g30.mp4": fonteCodigo(1920, 1080) });
    const r = await amb.ag.rodar(compose(amb, { f: "v1080_g30.mp4", dur: 400, rajada_ms: 2 }));
    expect(r.video).toEqual([1920, 1080]);
    expect(r.canvas).toEqual([1280, 720]);
    for (const l of [r.so_drawimage, r.completo, r.completo_sync, r.so_drawimage_sync, r.um_desenho_com_lupa, r.um_desenho_sem_lupa, r.um_desenho_sem_lupa_sync]) {
      expect(l.rafs).toBeGreaterThan(5);
      expect(l.fps_raf).toBeGreaterThan(50);
      expect(l.video[0]?.rvfc).toBe(0);
      expect(l.video[0]?.avanco_s).toBeGreaterThan(0);
    }
    expect(r.completo.dur_ms).toBeGreaterThanOrEqual(400);
    expect(r.rajada_por_s).toBeGreaterThan(0);
    // tocou de 0,5 s por ~2,6 s de laços: quadro entre 15 e 120
    expect(r.quadro_lido_fim).toBeGreaterThan(15);
    expect(r.quadro_lido_fim).toBeLessThan(120);
    expect(amb.vivos.size).toBe(0);
  });

  it("mídia ausente falha (MediaError) e não deixa vídeo vivo", async () => {
    const amb = ambienteFalso({});
    await expect(amb.ag.rodar(compose(amb, { f: "nao.mp4", dur: 100 }))).rejects.toThrow(/MediaError 4/);
    expect(amb.vivos.size).toBe(0);
  });
});

describe("switch", () => {
  it("B pausado no ponto de entrada assume no quadro exato", async () => {
    const amb = ambienteFalso({ "cam1.mp4": fonteCodigo(1920, 1080), "cam2.mp4": fonteCodigo(1920, 1080) });
    const r = await amb.ag.rodar(trocaDeClipe(amb, { reps: 4 }));
    expect(r.trocas).toHaveLength(4);
    expect(r.trocas.map((t) => t.quadro_entrada_esperado)).toEqual([360, 90, 201, 312]);
    for (const t of r.trocas) {
      expect(t.quadro_entrada_lido).toBe(t.quadro_entrada_esperado);
      expect(t.passou_do_ponto_ms).toBeGreaterThanOrEqual(0);
      expect(t.primeiro_quadro_novo_ms).toBeNull(); // sem rVFC, como no WebKitGTK
    }
    expect(r.resumo?.tempo_andou_ms?.n).toBe(4);
    expect(amb.vivos.size).toBe(0);
  });

  it("velb: velocidade do próximo definida pausado, alternando 1 e velb", async () => {
    const amb = ambienteFalso({ "cam1.mp4": fonteCodigo(1920, 1080), "cam2.mp4": fonteCodigo(1920, 1080) });
    const r = await amb.ag.rodar(trocaDeClipe(amb, { reps: 3, velb: 0.5 }));
    expect(r.velb).toBe(0.5);
    expect(r.trocas.map((t) => t.vel_do_proximo)).toEqual([1, 0.5, 1]);
    for (const t of r.trocas) expect(t.quadro_apos_definir_vel).toBe(t.quadro_esperado);
    // o 1º clipe depois da troca toca em 0,5 (definido antes do laço)
    expect(r.trocas[0]?.vel_pedida_do_atual).toBe(0.5);
    expect(r.trocas[0]?.vel_efetiva_do_atual).toBeCloseTo(0.5, 2);
  });
});

// ---- um clique ----

const NOMES = ["r0.mp4", "cp_r3_mkv.mp4", "cam1.mp4", "cam2.mp4", "cam1_p720.mp4", "cam2_p720.mp4", "v1080_g30.mp4"];
const FONTES: Record<string, FonteFalsa> = {
  "r0.mp4": fonteR(0),
  "cp_r3_mkv.mp4": fonteR(0.021),
  ...cams("", 1920, 1080),
  ...cams("_p720", 1280, 720),
  "v1080_g30.mp4": fonteCodigo(1920, 1080),
};
const MIDIAS: MidiasDiagnostico = {
  pasta: "/cache/edicao/diagnostico",
  ffmpeg: "ffmpeg version n9.0.1",
  midias: NOMES.map((nome) => ({ nome, caminho: "/cache/edicao/diagnostico/" + nome, p0_s: nome.startsWith("cp_") ? 0.021 : 0, inicio_s: 0 })),
  ambiente: { so: "linux", arch: "x86_64", webview: "2.52.6", sicro: "6.0.0", variaveis: { WEBKIT_DISABLE_DMABUF_RENDERER: "1" } },
};
const p0 = (n: string) => MIDIAS.midias.find((m) => m.nome === n)?.p0_s;
const UA_WEBKIT = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";

/** Plano curto: uma variante de cada medida, com os tempos reduzidos. */
const PLANO_CURTO: Etapa[] = [
  { medida: "tempo3", chave: "copias", rodar: (amb, m) => tempo3(amb, ["cp_r3_mkv.mp4"], [m.midias.find((x) => x.nome === "cp_r3_mkv.mp4")?.p0_s]) },
  { medida: "pool8", chave: "p720", rodar: (amb) => pool8(amb, { suf: "_p720", ciclos: 1, dur: 300, base_ms: 200 }) },
  { medida: "compose", chave: "v1080_g30", rodar: (amb) => compose(amb, { f: "v1080_g30.mp4", dur: 200, rajada_ms: 2 }) },
  { medida: "switch", chave: "padrao", rodar: (amb) => trocaDeClipe(amb, { reps: 2 }) },
];

/** Fontes acrescentadas por um teste (travadas). */
const FONTES_EXTRA: Record<string, FonteFalsa> = {};

function deps(plano: Etapa[]) {
  const amb = ambienteFalso(new Proxy(FONTES, { get: (t, k: string) => FONTES_EXTRA[k] ?? t[k] }));
  const salvos: Relatorio[] = [];
  const progresso: Parameters<NonNullable<Dependencias["aoProgresso"]>>[0][] = [];
  const d: Dependencias = {
    midias: async (av) => {
      av(0, 25);
      av(25, 25);
      return MIDIAS;
    },
    ambiente: () => amb,
    salvar: async (j) => {
      salvos.push(JSON.parse(JSON.stringify(j)) as Relatorio);
      return "/cache/edicao/diagnostico-2026-11-14-031245.json";
    },
    navegador: () => ({ ua: UA_WEBKIT, plataforma: "Linux x86_64", nucleos: 12, memoria_gb: null, tela: [2560, 1440], dpr: 1, idioma: "pt-BR" }),
    agora: () => new Date(2026, 10, 14, 3, 12, 45),
    aoProgresso: (p) => progresso.push(p),
    plano,
  };
  return { amb, d, salvos, progresso };
}

describe("diagnosticar (um clique)", () => {
  it("percorre as quatro medições e salva um JSON com tempo3, pool8, compose, switch, plataforma, motor e data", async () => {
    const { amb, d, salvos, progresso } = deps(PLANO_CURTO);
    const r = await amb.ag.rodar(diagnosticar(d));
    expect(r?.caminho).toMatch(/diagnostico-.*\.json$/);
    expect(salvos).toHaveLength(1);
    const j = salvos[0]!;
    for (const k of ["tempo3", "pool8", "compose", "switch", "plataforma", "motor", "data"]) expect(j).toHaveProperty(k);
    expect(j.data).toMatch(/^2026-11-14T03:12:45[+-]\d\d:\d\d$/);
    expect(j.motor).toEqual({ nome: "WebKitGTK", versao: "605.1.15", webview: "2.52.6" });
    expect(j.plataforma.so).toBe("linux");
    expect(j.plataforma.variaveis.WEBKIT_DISABLE_DMABUF_RENDERER).toBe("1");
    expect(j.ffmpeg).toBe("ffmpeg version n9.0.1");
    expect(j.midias.find((x) => x.nome === "cp_r3_mkv.mp4")).toEqual({ nome: "cp_r3_mkv.mp4", p0_s: 0.021, inicio_s: 0 });
    expect(j.sonda.canvas_manchado).toBe(false);
    expect(j.etapas.map((e) => `${e.medida}/${e.chave}/${e.ok}`)).toEqual(["tempo3/copias/true", "pool8/p720/true", "compose/v1080_g30/true", "switch/padrao/true"]);
    expect((j.tempo3.copias as { arquivos: { erros_previsto: number }[] }).arquivos[0]?.erros_previsto).toBe(0);
    // progresso: mídias com contagem, cada medida passa por rodando e termina feita
    expect(progresso.some((p) => p.midias?.[0] === 25)).toBe(true);
    const ult = progresso[progresso.length - 1]!;
    expect(ult.feitas).toBe(4);
    expect(ult.total).toBe(4);
    expect(Object.values(ult.estados)).toEqual(["feita", "feita", "feita", "feita", "feita"]);
    for (const m of ["tempo3", "pool8", "compose", "switch"] as const) expect(progresso.some((p) => p.estados[m] === "rodando")).toBe(true);
    // tabela curta
    const linhas = resumir(j, p0);
    expect(linhas.map((l) => l.medida)).toEqual(["tempo3", "pool8", "compose", "switch"]);
    expect(linhas[0]?.valores[0]).toEqual({ rotulo: "erros", valor: "0/11", ok: true });
    expect(linhas[1]?.valores.find((v) => v.rotulo === "entrada")).toEqual({ rotulo: "entrada", valor: "4/4", ok: true });
    expect(linhas[3]?.valores.find((v) => v.rotulo === "entrada")).toEqual({ rotulo: "entrada", valor: "2/2", ok: true });
    expect(amb.vivos.size).toBe(0);
  });

  it("etapa que falha fica no JSON e não para as outras", async () => {
    const plano: Etapa[] = [
      { medida: "tempo3", chave: "fontes", rodar: async () => { throw new Error("quebrou"); } },
      ...PLANO_CURTO.slice(3),
    ];
    const { amb, d, salvos, progresso } = deps(plano);
    await amb.ag.rodar(diagnosticar(d));
    const j = salvos[0]!;
    expect(String((j.tempo3.fontes as { erro: string }).erro)).toMatch(/quebrou/);
    expect(j.etapas.map((e) => e.ok)).toEqual([false, true]);
    expect(progresso[progresso.length - 1]?.estados.tempo3).toBe("falhou");
    expect(resumir(j, p0)[0]?.erro).toMatch(/quebrou/);
  });

  it("cancelar não salva nada", async () => {
    const { amb, d, salvos } = deps(PLANO_CURTO);
    const p = diagnosticar({ ...d, aoProgresso: (pr) => { if (pr.estados.pool8 === "rodando") amb.ag.cancelado = true; } });
    expect(await amb.ag.rodar(p)).toBeNull();
    expect(salvos).toHaveLength(0);
  });

  it("o plano completo roda inteiro e só pede mídias que o backend gera", async () => {
    // nomes que as receitas do Rust gravam
    const rs = readFileSync(new URL("../../../../src-tauri/src/commands/edicao_diagnostico_commands.rs", import.meta.url), "utf8");
    const corpo = rs.slice(rs.indexOf("fn receitas("), rs.indexOf("fn rodar("));
    const doBackend = new Set([...corpo.matchAll(/"([a-z0-9_]+\.(?:mp4|mkv|mov|ts))"/g)].map((x) => x[1]!));
    expect(doBackend.size).toBe(29);
    const p0De = (n: string) =>
      n.startsWith("cp_") ? 0.021 : n.startsWith("r3") ? 3 : n.startsWith("r007") ? 0.0667 : n.startsWith("r12h") ? 43200 : n === "r0.ts" ? 1.4213 : 0;
    const fontes: Record<string, FonteFalsa> = {};
    for (const n of doBackend) {
      fontes[n] = /^(r|cp_)/.test(n) ? fonteR(p0De(n)) : n.includes("_p720") ? fonteCodigo(1280, 720) : fonteCodigo(1920, 1080);
    }
    const midias: MidiasDiagnostico = { ...MIDIAS, midias: [...doBackend].map((nome) => ({ nome, caminho: "/c/" + nome, p0_s: p0De(nome), inicio_s: p0De(nome) })) };
    const amb = ambienteFalso(fontes, new Agenda(1));
    const pedidas = new Set<string>();
    const url = amb.url;
    amb.url = (n) => (pedidas.add(n), url(n));
    const salvos: Relatorio[] = [];
    const r = await amb.ag.rodar(
      diagnosticar({ ...deps(PLANO).d, midias: async () => midias, ambiente: () => amb, salvar: async (j) => (salvos.push(j), "/c/d.json"), plano: undefined }),
    );
    expect([...pedidas].filter((n) => !doBackend.has(n))).toEqual([]);
    expect(r?.caminho).toBe("/c/d.json");
    expect(salvos[0]?.etapas.map((e) => `${e.medida}/${e.chave}/${e.ok}`)).toEqual(PLANO.map((e) => `${e.medida}/${e.chave}/true`));
    for (const [m, ks] of Object.entries({ tempo3: ["fontes", "copias"], pool8: ["p720", "1080p", "p720_n2"], compose: ["v1080_g30", "cam1_p720"], switch: ["padrao", "mesmo_arquivo", "velb_0_5"] })) {
      expect(Object.keys(salvos[0]![m as "tempo3"])).toEqual(ks);
    }
    // cópias no modo previsto: alvo pelo p0 de cada uma
    expect((salvos[0]!.tempo3.copias as { arquivos: { erros_previsto: number }[] }).arquivos.map((a) => a.erros_previsto)).toEqual(Array(9).fill(0));
    expect(amb.vivos.size).toBe(0);
  });

  it("play que nunca resolve e vídeo parado viram falha da etapa; o resto roda e salva", async () => {
    const travado = { ...fonteCodigo(1920, 1080), trava: "play" as const };
    const parado = { ...fonteCodigo(1920, 1080), trava: "parado" as const };
    const { amb, d, salvos } = deps([
      { medida: "compose", chave: "v1080_g30", rodar: (a) => compose(a, { f: "travado.mp4", dur: 200, rajada_ms: 2 }) },
      { medida: "switch", chave: "mesmo_arquivo", rodar: (a) => trocaDeClipe(a, { fa: "parado.mp4", reps: 1 }) },
      ...PLANO_CURTO.slice(3),
    ]);
    Object.assign(FONTES_EXTRA, { "travado.mp4": travado, "parado.mp4": parado });
    try {
      await amb.ag.rodar(diagnosticar(d));
    } finally {
      for (const k of Object.keys(FONTES_EXTRA)) delete FONTES_EXTRA[k];
    }
    const j = salvos[0]!;
    expect(j.etapas.map((e) => e.ok)).toEqual([false, false, true]);
    expect(String((j.compose.v1080_g30 as { erro: string }).erro)).toMatch(/timeout play/);
    expect(String((j.switch.mesmo_arquivo as { erro: string }).erro)).toMatch(/timeout fronteira/);
    expect(amb.vivos.size).toBe(0);
  });

  it("salvar que falha devolve o relatório e o erro", async () => {
    const { amb, d } = deps(PLANO_CURTO.slice(3));
    const r = await amb.ag.rodar(diagnosticar({ ...d, salvar: async () => { throw { kind: "filesystem", message: "sem espaço" }; } }));
    expect(r?.caminho).toBeNull();
    expect(r?.erro).toBe("sem espaço");
    expect(r?.relatorio.switch).toHaveProperty("padrao");
  });

  it("etapa com a página oculta fica marcada", async () => {
    const { amb, d, salvos } = deps(PLANO_CURTO.slice(2));
    let ocultou = false;
    const plano: Etapa[] = [
      { ...PLANO_CURTO[2]!, rodar: async (a, m) => { ocultou = true; return PLANO_CURTO[2]!.rodar(a, m); } },
      PLANO_CURTO[3]!,
    ];
    amb.oculta = () => {
      const r = ocultou;
      ocultou = false;
      return r;
    };
    await amb.ag.rodar(diagnosticar({ ...d, plano }));
    expect(salvos[0]?.etapas.map((e) => e.oculta)).toEqual([true, false]);
  });
});

describe("motor e data", () => {
  it("reconhece WebKitGTK, WebView2 e Chromium pelo UA", () => {
    expect(motorDoWebview(UA_WEBKIT)).toEqual({ nome: "WebKitGTK", versao: "605.1.15" });
    expect(
      motorDoWebview("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.2903.70"),
    ).toEqual({ nome: "WebView2", versao: "131.0.2903.70" });
    expect(motorDoWebview("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/154.0.0.0 Safari/537.36")).toEqual({
      nome: "Chromium",
      versao: "154.0.0.0",
    });
    expect(motorDoWebview("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)").nome).toBe("WebKit");
  });

  it("data em ISO com o fuso local", () => {
    expect(isoLocal(new Date(2026, 0, 2, 3, 4, 5))).toMatch(/^2026-01-02T03:04:05[+-]\d\d:\d\d$/);
  });
});

describe("tempo3 com mídia recusada", () => {
  it("não busca no elemento com erro e segue para o próximo arquivo", async () => {
    const amb = ambienteFalso({ "r0.mp4": fonteR(0), "r0.mkv": fonteR(0) });
    const r = await amb.ag.rodar(tempo3(amb, ["r0.ts", "r0.mkv"]));
    expect(r.arquivos.map((a) => a.arquivo)).toEqual(["r0.ts", "r0.mkv"]);
    expect(r.arquivos[0]?.pulado).toBe("erro");
    expect(r.arquivos[0]?.erro).toBe(4);
    expect(r.arquivos[0]?.buscas).toBeUndefined();
    expect(baseDeTempo(r.arquivos[1]!, 0)).toBe("absoluto");
    expect(amb.vivos.size).toBe(0);
  });

  it("elemento sem metadados (MP4 de 12 h no WebKitGTK) também é pulado", async () => {
    const amb = ambienteFalso({ "r0.mp4": fonteR(0), "r12h.mp4": { ...fonteR(43200), trava: "carga" } });
    const r = await amb.ag.rodar(tempo3(amb, ["r12h.mp4"]));
    expect(r.arquivos[0]?.meta).toBe("timeout");
    expect(r.arquivos[0]?.pulado).toBe("sem_dados");
    expect(r.arquivos[0]?.buscas).toBeUndefined();
    expect(amb.vivos.size).toBe(0);
  });
});
