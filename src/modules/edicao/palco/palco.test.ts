/** Lógica do palco: vista, transporte (faixa e bordas), busca um-por-vez, quadro final e alças. */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Caixa, EfEnquadramento, EfGirar, EfLupa, EfRegua, EfVelocidadeExibida, PreviaMidia, TabelaQuadros } from "@domain/edicao";
import { VISTA_INICIAL, caixaMedida, contido, daTela, deslocar, limitar, mostrado, paraTela, zoomEm, zoomMaximo } from "./vista";
import { aparar, bordaArrastada, entradaNoCursor, faixaDoClipe, fronteira, marcaLivre, marcarBloqueado, meioDoQuadro, saidaNoCursor, tempoDaFronteira, tempoDoX, xDoTempo } from "./faixa";
import { Buscador, ESPERA_MS, type Agenda } from "./motor/busca";
import type { ElementoBusca } from "./motor/elemento";
import { relogioMidia } from "./motor/relogioMidia";
import { QuadroFiel, type Fiel } from "./quadroFiel";
import { aDaGirada, aDaOrigem, alcaDaAncora, alcaDeLugar, alcaDoLugarFixo, alcaNaOrigem, alcasDaRegiao, alcasDeCaixa, alcasDePonto, caixaArrastada, comRegiao, proporcaoDaSaida } from "./alcas";
import { _paraTeste } from "./Palco";
import { caixaComProporcao } from "./camadaEdicao";
import { aDe, ponto } from "../modelo/geometria";
import { exibido, quadroEm, tempoDoQuadro } from "../modelo/tempo";

/** 30 qps em 1/15360, começando em `pts0`. */
const tab = (n: number, pts0 = 0, passo = 512): TabelaQuadros => ({ tb: { num: 1, den: 15360 }, pts0, passos: Array(n - 1).fill(passo), passo_mediano: passo, chaves: [0], vfr: false, origem: "decodificador" });
const perto = (a: number, b: number, e = 1e-12) => expect(Math.abs(a - b)).toBeLessThan(e);

describe("vista: zoom e deslocamento da tela", () => {
  const caixa = { w: 800, h: 600 };
  const saida = { w: 1920, h: 1080 };
  const img = contido(caixa, saida);

  it("a saída fica contida e centrada", () => {
    expect(img).toEqual({ x: 0, y: 75, w: 800, h: 450 });
    expect(contido({ w: 0, h: 10 }, saida)).toEqual({ x: 0, y: 0, w: 0, h: 0 });
  });

  it("zoom com o ponto parado; nunca abaixo de 1×; deslocamento limitado", () => {
    const p = { x: 200, y: 300 };
    const antes = daTela(VISTA_INICIAL, img, p);
    const v = zoomEm(VISTA_INICIAL, 2, p, caixa, img);
    expect(v.s).toBe(2);
    const depois = daTela(v, img, p);
    perto(depois.x, antes.x);
    perto(depois.y, antes.y);
    expect(zoomEm(v, 0.1, p, caixa, img)).toEqual(limitar(VISTA_INICIAL, caixa, img));
    // maior que a caixa: sem borda sobrando
    const longe = deslocar(v, 5000, 5000, caixa, img);
    const m = mostrado(longe, img);
    expect(m.x).toBeLessThanOrEqual(0);
    expect(m.x + m.w).toBeGreaterThanOrEqual(caixa.w);
    // menor que a caixa no eixo vertical (450·1,2 < 600): fica centrada
    const v12 = limitar({ s: 1.2, tx: 0, ty: 999 }, caixa, img);
    const m12 = mostrado(v12, img);
    perto(m12.y + m12.h / 2, caixa.h / 2);
  });

  it("tela ↔ canvas vão e voltam; teto do zoom pela saída", () => {
    const v = zoomEm(VISTA_INICIAL, 3, { x: 500, y: 200 }, caixa, img);
    const q = { x: 0.31, y: 0.77 };
    const t = paraTela(v, img, q);
    const de = daTela(v, img, t);
    perto(de.x, q.x);
    perto(de.y, q.y);
    expect(zoomMaximo(img, saida)).toBe(Math.max(16, (40 * 1920) / 800));
    expect(zoomEm(VISTA_INICIAL, 1e6, { x: 0, y: 0 }, caixa, img, 16).s).toBe(16);
  });

  it("palco escondido mede 0×0: a caixa anterior vale e a vista não perde o deslocamento (§8.1)", () => {
    const v = deslocar(zoomEm(VISTA_INICIAL, 4, { x: 600, y: 400 }, caixa, img), -37, -21, caixa, img);
    const escondida = caixaMedida(caixa, 0, 0);
    expect(escondida).toBe(caixa);
    expect(limitar(v, escondida, contido(escondida, saida))).toEqual(v);
    // sem a guarda a vista iria para o canto
    expect(limitar(v, { w: 0, h: 0 }, contido({ w: 0, h: 0 }, saida))).not.toEqual(v);
    // medida de verdade troca; igual devolve a mesma referência
    expect(caixaMedida(caixa, 640, 360)).toEqual({ w: 640, h: 360 });
    expect(caixaMedida(caixa, caixa.w, caixa.h)).toBe(caixa);
  });
});

describe("transporte: faixa da mídia inteira, bordas e I/O", () => {
  const t = tab(300, 30720); // começa em 2 s
  const clipe = { inicio_s: tempoDoQuadro(t, 30), inicio_pts: 30720 + 30 * 512, fim_s: tempoDoQuadro(t, 90), fim_pts: 30720 + 90 * 512 };

  it("x ↔ tempo, limitado à faixa", () => {
    const e = { de: 2, ate: 12, largura: 1000 };
    expect(xDoTempo(e, 7)).toBe(500);
    expect(tempoDoX(e, 250)).toBe(4.5);
    expect(tempoDoX(e, -10)).toBe(2);
    expect(tempoDoX(e, 5000)).toBe(12);
  });

  it("fronteira de quadro mais próxima; a do fim da mídia não tem pts", () => {
    expect(fronteira(t, tempoDoQuadro(t, 10) + 0.004)).toBe(10);
    expect(fronteira(t, tempoDoQuadro(t, 10) + 0.03)).toBe(11);
    expect(fronteira(t, 99)).toBe(300);
    expect(tempoDaFronteira(t, 300)).toEqual({ t: (30720 + 300 * 512) / 15360, pts: null });
  });

  it("meio do quadro = o do relógio da prévia, e o quadro exibido nele é o próprio", () => {
    const previa: PreviaMidia = { relativo: "x", tipo: "copia", desloc_s: 1.979, de_s: 2, ate_s: 12, largura: 640, altura: 360, pronta: true };
    const r = relogioMidia(previa, t);
    for (const j of [0, 1, 150, 299]) {
      expect(meioDoQuadro(t, j)).toBe(r.meio(j));
      expect(quadroEm(t, { t: meioDoQuadro(t, j) }).indice).toBe(j);
      expect(r.quadro(r.alvo(j))).toBe(j);
    }
  });

  it("aparar pelas bordas: preso à fronteira; nunca vazio", () => {
    expect(faixaDoClipe(clipe, t)).toEqual([30, 90]);
    expect(aparar(clipe, "inicio", 40, t)).toEqual({ inicio_s: tempoDoQuadro(t, 40), inicio_pts: 30720 + 40 * 512 });
    expect(aparar(clipe, "inicio", 90, t)).toBeNull();
    expect(aparar(clipe, "inicio", 30, t)).toBeNull();
    expect(aparar(clipe, "fim", 30, t)).toBeNull();
    expect(aparar(clipe, "fim", 300, t)).toEqual({ fim_s: (30720 + 300 * 512) / 15360, fim_pts: null });
  });

  it("borda arrastada: anda o que o ponteiro andou; pegar e soltar sem mover não muda nada", () => {
    const pista = [2, 12] as const;
    // pegou a borda do fim 0,2 s ao lado dela e não moveu
    expect(bordaArrastada(clipe, "fim", clipe.fim_s + 0.2, clipe.fim_s + 0.2, t, pista)).toBeNull();
    expect(bordaArrastada(clipe, "inicio", clipe.inicio_s - 0.2, clipe.inicio_s - 0.2, t, pista)).toBeNull();
    // andou 10 quadros para a direita, a partir de onde pegou
    const dez = 10 * (512 / 15360);
    expect(bordaArrastada(clipe, "fim", clipe.fim_s + 0.2, clipe.fim_s + 0.2 + dez, t, pista)).toEqual({ fim_s: tempoDoQuadro(t, 100), fim_pts: 30720 + 100 * 512 });
    expect(bordaArrastada(clipe, "inicio", clipe.inicio_s - 0.2, clipe.inicio_s - 0.2 + dez, t, pista)).toEqual({ inicio_s: tempoDoQuadro(t, 40), inicio_pts: 30720 + 40 * 512 });
    // ponteiro na ponta da pista: ponta da mídia, mesmo pegando fora do centro
    expect(bordaArrastada(clipe, "fim", clipe.fim_s + 0.2, 12, t, pista)).toEqual({ fim_s: (30720 + 300 * 512) / 15360, fim_pts: null });
    expect(bordaArrastada(clipe, "inicio", clipe.inicio_s + 0.2, 2, t, pista)).toEqual({ inicio_s: 2, inicio_pts: 30720 });
    // ponteiro já na ponta da pista ao pegar, sem mover: nada (não salta para a ponta da mídia)
    expect(bordaArrastada(clipe, "fim", 12, 12, t, pista)).toBeNull();
    expect(bordaArrastada(clipe, "inicio", 2, 2, t, pista)).toBeNull();
    // nunca vazio
    expect(bordaArrastada(clipe, "fim", clipe.fim_s, 2, t, pista)).toBeNull();
  });

  it("I grava o quadro exibido; O inclui o quadro exibido (fim = o seguinte); os dois ALARGAM o clipe (contratos (h) 21)", () => {
    // cursor antes do clipe: I puxa o início para trás
    expect(entradaNoCursor(clipe, t, meioDoQuadro(t, 12))).toEqual({ inicio_s: tempoDoQuadro(t, 12), inicio_pts: 30720 + 12 * 512 });
    // cursor depois do fim: O empurra o fim; o quadro 140 fica DENTRO (fim exclusivo = 141)
    expect(saidaNoCursor(clipe, t, meioDoQuadro(t, 140))).toEqual({ fim_s: tempoDoQuadro(t, 141), fim_pts: 30720 + 141 * 512 });
    // dentro: encurta, com o quadro mostrado incluído
    expect(saidaNoCursor(clipe, t, meioDoQuadro(t, 60))).toEqual({ fim_s: tempoDoQuadro(t, 61), fim_pts: 30720 + 61 * 512 });
    // O no último quadro do clipe: o fim já é o seguinte, nada muda
    expect(saidaNoCursor(clipe, t, meioDoQuadro(t, 89))).toBeNull();
    // O no primeiro quadro: fica um quadro só
    expect(saidaNoCursor(clipe, t, meioDoQuadro(t, 30))).toEqual({ fim_s: tempoDoQuadro(t, 31), fim_pts: 30720 + 31 * 512 });
    // último quadro da mídia: até o fim (pts nulo)
    expect(saidaNoCursor(clipe, t, meioDoQuadro(t, 299))).toEqual({ fim_s: (30720 + 300 * 512) / 15360, fim_pts: null });
    // O antes do início ou I depois do fim: nada
    expect(saidaNoCursor(clipe, t, meioDoQuadro(t, 10))).toBeNull();
    expect(saidaNoCursor(clipe, t, meioDoQuadro(t, 29))).toBeNull();
    expect(entradaNoCursor(clipe, t, meioDoQuadro(t, 95))).toBeNull();
  });

  it("marcar só com MidiaInfo.marcavel (contratos (h) 17): a extensão do arquivo não conta", () => {
    const pac = { ...t, origem: "pacotes" as const };
    expect(marcarBloqueado({ marcavel: false }, pac)).toBe(true);
    expect(marcarBloqueado({ marcavel: false }, t)).toBe(true);
    expect(marcarBloqueado({ marcavel: true }, pac)).toBe(false);
    expect(marcarBloqueado({ marcavel: true }, t)).toBe(false);
    expect(marcarBloqueado(null, t)).toBe(true);
    expect(marcarBloqueado({ marcavel: true }, null)).toBe(true);
  });

  it("marca só com a imagem do quadro na tela: busca em curso (troca de clipe) bloqueia (contratos (h) 27)", () => {
    const ok = { marcavel: true };
    // troca de clipe: jRef já no quadro novo (40), a imagem ainda é a do 7
    expect(marcaLivre("trecho", ok, t, { alvo: 40, ultimo: 7 }, 40)).toBe(false);
    // pintou o 40, sem nada pendente
    expect(marcaLivre("trecho", ok, t, { alvo: null, ultimo: 40 }, 40)).toBe(true);
    // passo em curso com outro pendente
    expect(marcaLivre("trecho", ok, t, { alvo: 41, ultimo: 40 }, 40)).toBe(false);
    // nada pintado ainda, ou sem elemento
    expect(marcaLivre("trecho", ok, t, { alvo: null, ultimo: null }, 40)).toBe(false);
    expect(marcaLivre("congelar", ok, t, null, 40)).toBe(false);
    // mídia não marcável
    expect(marcaLivre("trecho", { marcavel: false }, t, { alvo: null, ultimo: 40 }, 40)).toBe(false);
    // clipe sem fonte: não depende de busca
    expect(marcaLivre("titulo", null, null, null, null)).toBe(true);
    expect(marcaLivre(null, ok, t, { alvo: null, ultimo: 40 }, 40)).toBe(false);
  });
});

describe("busca: um seek por vez, alvo pendente, pinta no seeked + rAF", () => {
  function falso() {
    const ouvintes = new Set<() => void>();
    const pedidos: number[] = [];
    const estado = { buscando: false };
    const el: ElementoBusca = {
      buscar: (j) => void pedidos.push(j),
      quadro: () => pedidos.at(-1) ?? 0,
      agora: () => 0,
      buscando: () => estado.buscando,
      aoBuscado: (f) => (ouvintes.add(f), () => ouvintes.delete(f)),
      tocar: async () => {},
      pausar: () => {},
      pausado: () => true,
    };
    const rafs: (() => void)[] = [];
    const timers = new Map<number, () => void>();
    let n = 0;
    const agenda: Agenda = {
      raf: (f) => void rafs.push(f),
      setTimeout: (f) => (timers.set(++n, f), n),
      clearTimeout: (h) => void timers.delete(h as number),
    };
    const buscado = () => ouvintes.forEach((f) => f());
    const quadro = () => rafs.splice(0).forEach((f) => f());
    return { el, pedidos, agenda, buscado, quadro, timers, ouvintes, estado };
  }

  it("pedidos durante uma busca viram UM pendente (o último); pinta só depois do rAF", () => {
    const f = falso();
    const pintados: number[] = [];
    const b = new Buscador(f.el, (j, final) => final && pintados.push(j), f.agenda);
    b.ir(10);
    b.ir(11);
    b.ir(12);
    expect(f.pedidos).toEqual([10]);
    expect(b.alvo).toBe(12);
    f.buscado();
    expect(pintados).toEqual([]);
    f.quadro();
    // a primeira pintura não é a final: o pendente espera
    expect(pintados).toEqual([]);
    expect(f.pedidos).toEqual([10]);
    f.quadro();
    expect(pintados).toEqual([10]);
    expect(f.pedidos).toEqual([10, 12]);
    f.buscado();
    f.quadro();
    f.quadro();
    expect(pintados).toEqual([10, 12]);
    expect(b.alvo).toBeNull();
    expect(b.ultimo).toBe(12);
    b.fechar();
    expect(f.ouvintes.size).toBe(0);
  });

  it("sem `seeked` (busca para o mesmo instante) a fila não trava", () => {
    const f = falso();
    const pintados: number[] = [];
    const todas: number[] = [];
    const b = new Buscador(f.el, (j, final) => (todas.push(j), final && pintados.push(j)), f.agenda);
    b.ir(5);
    b.ir(6);
    expect(f.timers.size).toBe(1);
    // o vigia de ESPERA_MS dispara
    [...f.timers.values()][0]!();
    f.quadro();
    f.quadro();
    expect(pintados).toEqual([5]);
    expect(todas).toEqual([5, 5]);
    expect(f.pedidos).toEqual([5, 6]);
    expect(ESPERA_MS).toBeGreaterThan(0);
  });

  it("busca lenta (ainda `seeking` no fim da espera): nada é pintado até o `seeked`", () => {
    const f = falso();
    const todas: number[] = [];
    const b = new Buscador(f.el, (j) => void todas.push(j), f.agenda);
    f.estado.buscando = true;
    b.ir(7);
    // o vigia dispara duas vezes com o elemento ainda buscando: rearma, não pinta
    for (let i = 0; i < 2; i++) {
      const [h, vigia] = [...f.timers.entries()][0]!;
      f.timers.delete(h);
      vigia();
      f.quadro();
      f.quadro();
      expect(todas).toEqual([]);
      expect(f.timers.size).toBe(1);
      expect(b.alvo).toBe(7);
    }
    f.estado.buscando = false;
    f.buscado();
    f.quadro();
    f.quadro();
    expect(todas).toEqual([7, 7]);
    expect(f.timers.size).toBe(0);
    expect(b.alvo).toBeNull();
  });
});

describe("quadro final (D22, §7.2)", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  function montar(falhar = false) {
    const estados: Fiel[] = [];
    const pedidos: number[] = [];
    let resolver: ((s: string) => void) | null = null;
    const f = new QuadroFiel(
      (q) => {
        pedidos.push(q);
        return falhar ? Promise.reject(new Error("x")) : new Promise<string>((r) => (resolver = r));
      },
      (e) => estados.push(e),
      { agora: () => Date.now(), setTimeout: (g, ms) => setTimeout(g, ms), clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>) },
    );
    return { f, estados, pedidos, responder: (s: string) => resolver?.(s) };
  }

  it("300 ms depois de pausar pede o slot; a imagem fica até tocar", async () => {
    const m = montar();
    m.f.pausou(40);
    await vi.advanceTimersByTimeAsync(299);
    expect(m.pedidos).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    expect(m.pedidos).toEqual([40]);
    expect(m.f.estado).toBe("pedindo");
    m.responder("blob:40");
    await vi.advanceTimersByTimeAsync(0);
    expect(m.f.estado).toEqual({ q: 40, src: "blob:40" });
    m.f.tocou();
    expect(m.f.estado).toBe("viva");
  });

  it("passo de quadro nos últimos 600 ms adia o pedido; a resposta velha não vale", async () => {
    const m = montar();
    m.f.passo(10);
    await vi.advanceTimersByTimeAsync(400);
    m.f.passo(11);
    await vi.advanceTimersByTimeAsync(599);
    expect(m.pedidos).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    expect(m.pedidos).toEqual([11]);
    m.f.passo(12);
    m.responder("blob:11");
    await vi.advanceTimersByTimeAsync(0);
    expect(m.f.estado).toBe("viva");
  });

  it("editar repete o pedido sem voltar à viva; botão pede na hora; falha volta à viva", async () => {
    const m = montar();
    m.f.pedirJa(7);
    expect(m.pedidos).toEqual([7]);
    m.responder("a");
    await vi.advanceTimersByTimeAsync(0);
    m.f.editou();
    expect(m.f.estado).toEqual({ q: 7, src: "a" });
    await vi.advanceTimersByTimeAsync(300);
    expect(m.pedidos).toEqual([7, 7]);
    expect(m.f.estado).toEqual({ q: 7, src: "a" });
    const r = montar(true);
    r.f.pedirJa(3);
    await vi.advanceTimersByTimeAsync(0);
    expect(r.f.estado).toBe("viva");
  });
});

describe("alças", () => {
  const c: Caixa = { x: 0.2, y: 0.2, w: 0.4, h: 0.2 };

  it("canto, borda, mover, Shift (proporção) e Alt (do centro)", () => {
    const canto = caixaArrastada(c, [1, 1], { x: 0.6, y: 0.4 }, { x: 0.7, y: 0.5 }, { shift: false, alt: false }, null);
    [canto.x, canto.y, canto.w, canto.h].forEach((v, i) => perto(v, [0.2, 0.2, 0.5, 0.3][i]!, 1e-12));
    expect(caixaArrastada(c, [0, -1], { x: 0.4, y: 0.2 }, { x: 0.4, y: 0.1 }, { shift: false, alt: false }, null)).toMatchObject({ x: 0.2, w: 0.4 });
    const mv = caixaArrastada(c, [0, 0], { x: 0.4, y: 0.3 }, { x: 0.9, y: 0.3 }, { shift: false, alt: false }, null);
    expect(mv.x + mv.w).toBeLessThanOrEqual(1);
    const prop = caixaArrastada(c, [1, 1], { x: 0.6, y: 0.4 }, { x: 0.8, y: 0.45 }, { shift: true, alt: false }, 2);
    perto(prop.w / prop.h, 2, 1e-9);
    const alt = caixaArrastada(c, [1, 0], { x: 0.6, y: 0.3 }, { x: 0.7, y: 0.3 }, { shift: false, alt: true }, null);
    perto(alt.x + alt.w / 2, c.x + c.w / 2, 1e-12);
    // proporção na borda: encolhe os dois lados, o canto fixo fica
    const borda = caixaArrastada({ x: 0.5, y: 0.5, w: 0.2, h: 0.1 }, [1, 1], { x: 0.7, y: 0.6 }, { x: 0.99, y: 0.95 }, { shift: true, alt: false }, 2);
    perto(borda.w / borda.h, 2, 1e-9);
    expect([borda.x, borda.y]).toEqual([0.5, 0.5]);
    expect(borda.x + borda.w).toBeLessThanOrEqual(1);
    // menor que o mínimo: fica como estava
    expect(caixaArrastada(c, [1, 1], { x: 0.6, y: 0.4 }, { x: 0.2, y: 0.2 }, { shift: false, alt: false }, null)).toEqual(c);
  });

  it("caixa de rastreio com giro de 90°: alça na tela, âncora no espaço da fonte", () => {
    const girar: EfGirar = { id: "g", ligado: true, nome: "", ativo: null, tipo: "girar", graus: 90, espelhar_h: false, espelhar_v: false };
    const A = aDe([girar], 0, { w: 1920, h: 1080 }, { w: 1080, h: 1920 });
    const alcas = alcasDeCaixa("r1", c, A, 4.5, 2304);
    expect(alcas.map((a) => a.id.split(":").pop())).toEqual(["no", "ne", "se", "so", "n", "l", "s", "o", "mover"]);
    const mover = alcas.find((a) => a.tipo === "mover")!;
    const de = mover.em;
    const para = { x: de.x + 0.1, y: de.y };
    const m = mover.arrastar(de, para, { shift: false, alt: false });
    expect("rastreio" in m && m.rastreio).toBe("r1");
    if (!("ancora" in m)) throw new Error("sem âncora");
    expect([m.ancora.t, m.ancora.pts]).toEqual([4.5, 2304]);
    // o que andou na tela, levado de volta pela inversa de A, é o que andou na fonte
    const novo = ponto(A, { x: m.ancora.v.x + m.ancora.v.w / 2, y: m.ancora.v.y + m.ancora.v.h / 2 });
    perto(novo.x, para.x, 1e-9);
    perto(novo.y, para.y, 1e-9);
    const pt = alcasDePonto("r2", { x: 0.5, y: 0.5 }, A, 1, null)[0]!;
    expect(pt.arrastar(pt.em, pt.em, { shift: false, alt: false })).toEqual({ rastreio: "r2", ancora: { t: 1, pts: null, v: { x: 0.5, y: 0.5, w: 0, h: 0 } } });
  });

  it("região do enquadramento: fixa troca o valor, animada ganha a chave do instante", () => {
    const e: EfEnquadramento = { id: "q", ligado: true, nome: "", ativo: null, tipo: "enquadramento", regiao: { x: 0, y: 0, w: 1, h: 1 }, ajuste: "conter", interpolacao: "bilinear" };
    const nova = { x: 0.1, y: 0.1, w: 0.5, h: 0.5 };
    expect(comRegiao(e, nova, 2, 60).regiao).toEqual(nova);
    const anim = { ...e, regiao: [{ t: 1, pts: 30, v: { x: 0, y: 0, w: 1, h: 1 } }, { t: 3, pts: 90, v: { x: 0, y: 0, w: 1, h: 1 } }] };
    const r = comRegiao(anim, nova, 2, 60).regiao;
    expect(Array.isArray(r) && r.map((k) => k.t)).toEqual([1, 2, 3]);
    expect(Array.isArray(r) && r[1]).toEqual({ t: 2, pts: 60, v: nova });
    // a alça de canto da região guarda a proporção da saída
    const g = aDaGirada([], { w: 1920, h: 1080 }, { w: 1920, h: 1080 });
    const prop = proporcaoDaSaida([], { w: 1920, h: 1080 }, { w: 1080, h: 1080 })!;
    const regiao = { x: 0.1, y: 0.1, w: 0.3, h: 0.3 / prop };
    const se = alcasDaRegiao(e, regiao, 2, 60, g, prop).find((a) => a.id.endsWith(":se"))!;
    const m = se.arrastar(se.em, { x: se.em.x + 0.05, y: se.em.y + 0.01 }, { shift: false, alt: false });
    if (!("efeito" in m) || m.efeito.tipo !== "enquadramento" || Array.isArray(m.efeito.regiao)) throw new Error("mudança errada");
    perto((m.efeito.regiao.w * 1920) / (m.efeito.regiao.h * 1080), 1, 1e-9);
  });

  it("vista de origem: o giro troca os lados; sem giro, a origem inteira contida", () => {
    expect(aDaGirada([], { w: 1920, h: 1080 }, { w: 1920, h: 1080 })).toEqual([1, 0, 0, 1, 0, 0]);
    const girar: EfGirar = { id: "g", ligado: true, nome: "", ativo: null, tipo: "girar", graus: 90, espelhar_h: false, espelhar_v: false };
    const g = aDaGirada([girar], { w: 1920, h: 1080 }, { w: 1920, h: 1080 });
    perto(g[0], (1080 * 1080) / (1920 * 1920));
    // a origem girada inteira: o canto (0,0) da fonte cai na borda do encaixe
    const A = aDaOrigem([girar], { w: 1920, h: 1080 }, { w: 1920, h: 1080 });
    const cantos = [ponto(A, { x: 0, y: 0 }), ponto(A, { x: 1, y: 1 })];
    for (const p of cantos) expect(p.y === 0 || p.y === 1).toBe(true);
  });

  it("lugar de texto: arrastar move o efeito", () => {
    const e = { id: "t", ligado: true, nome: "", ativo: null, tipo: "texto" as const, conteudo: "A", estilo: { fonte: "sans" as const, negrito: false, italico: false, tamanho: 0.05, cor: "#fff", alinhamento: "centro" as const, caixa: null, contorno: null }, lugar: { x: 0.5, y: 0.9, h: "centro" as const, v: "base" as const }, largura_max: 0.8 };
    const a = alcaDeLugar(e);
    const m = a.arrastar({ x: 0.5, y: 0.9 }, { x: 0.6, y: 1.2 }, { shift: false, alt: false });
    const l = "efeito" in m && m.efeito.tipo === "texto" ? m.efeito.lugar : null;
    expect(l).toMatchObject({ y: 1, h: "centro", v: "base" });
    perto(l!.x, 0.6, 1e-12);
  });

  it("ferramenta: caixa com a proporção pedida em px da saída", () => {
    const r = caixaComProporcao({ x: 0.1, y: 0.1 }, { x: 0.5, y: 0.2 }, 16 / 9, { w: 1920, h: 1080 });
    perto((r.w * 1920) / (r.h * 1080), 16 / 9, 1e-9);
    expect(caixaComProporcao({ x: 0.5, y: 0.5 }, { x: 0.2, y: 0.4 }, undefined, { w: 1, h: 1 })).toEqual({ x: 0.2, y: 0.4, w: 0.3, h: 0.09999999999999998 });
    void exibido;
  });
});

describe("alças da Fase 1: âncora da régua, centro da lupa fixa e lugar da velocidade fixa", () => {
  // A: a fonte ocupa a metade direita do canvas
  const A: [number, number, number, number, number, number] = [0.5, 0, 0, 1, 0.5, 0];
  const mod = { shift: false, alt: false };

  it("âncora da régua: mostrada no canvas por A; o arrasto volta ao espaço de origem pela inversa", () => {
    const regua = { id: "rg", tipo: "regua", ancora: { x: 0.4, y: 0.6 } } as unknown as EfRegua;
    const [a] = alcaDaAncora(regua, A);
    expect(a!.id).toBe("ancora:rg");
    perto(a!.em.x, 0.7);
    perto(a!.em.y, 0.6);
    // 0,1 do canvas para a direita = 0,2 da fonte (A dobra o x)
    const m = a!.arrastar({ x: 0.7, y: 0.6 }, { x: 0.8, y: 0.5 }, mod) as { efeito: EfRegua };
    perto(m.efeito.ancora.x, 0.6);
    perto(m.efeito.ancora.y, 0.5);
    expect(m.efeito.id).toBe("rg");
    // limitada à fonte
    expect((a!.arrastar({ x: 0.7, y: 0.6 }, { x: 5, y: -5 }, mod) as { efeito: EfRegua }).efeito.ancora).toEqual({ x: 1, y: 0 });
    expect(alcaNaOrigem(a!)).toBe(true);
    // A sem inversa: sem alça
    expect(alcaDaAncora(regua, [0, 0, 0, 0, 0, 0])).toEqual([]);
  });

  it("lupa fixa e velocidade fixa: mover no canvas; as que acompanham não têm essa alça", () => {
    const lupa = { id: "lp", tipo: "lupa", rastreio: "r", onde: { modo: "fixa", x: 0.8, y: 0.25 } } as unknown as EfLupa;
    const [a] = alcaDoLugarFixo(lupa);
    expect([a!.id, a!.em]).toEqual(["centro:lp", { x: 0.8, y: 0.25 }]);
    const m = a!.arrastar({ x: 0.8, y: 0.25 }, { x: 0.5, y: 0.5 }, mod) as { efeito: EfLupa };
    expect(m.efeito.onde.modo).toBe("fixa");
    perto((m.efeito.onde as { x: number }).x, 0.5);
    perto((m.efeito.onde as { y: number }).y, 0.5);
    expect(alcaNaOrigem(a!)).toBe(false);
    expect(alcaDoLugarFixo({ ...lupa, onde: { modo: "segue", dx: 0.1, dy: 0 } })).toEqual([]);
    const vel = { id: "ve", tipo: "velocidade_exibida", onde: { modo: "fixa", lugar: { x: 0.1, y: 0.9, h: "esq", v: "base" } } } as unknown as EfVelocidadeExibida;
    const [b] = alcaDoLugarFixo(vel);
    const n = b!.arrastar({ x: 0.1, y: 0.9 }, { x: 0.3, y: 2 }, mod) as { efeito: EfVelocidadeExibida };
    expect(n.efeito.onde).toEqual({ modo: "fixa", lugar: { x: expect.closeTo(0.3, 12), y: 1, h: "esq", v: "base" } });
    expect(alcaDoLugarFixo({ ...vel, onde: { modo: "segue", rastreio: "r", dx: 0, dy: 0 } })).toEqual([]);
    // a alça de `lugar` do texto também é do canvas
    expect(alcaNaOrigem({ id: "lugar:t" })).toBe(false);
    expect(alcaNaOrigem({ id: "caixa:r" })).toBe(true);
  });

  it("rastreio do efeito selecionado: destaque, lupa, ocultar, holofote e a velocidade que acompanha", () => {
    const r = _paraTeste.rastreioDoEfeito;
    expect(r({ tipo: "destaque", rastreio: "a" })).toBe("a");
    expect(r({ tipo: "lupa", rastreio: "b" })).toBe("b");
    expect(r({ tipo: "ocultar", alvo: { rastreio: "c" } })).toBe("c");
    expect(r({ tipo: "holofote", alvo: { rastreio: "d" } })).toBe("d");
    expect(r({ tipo: "velocidade_exibida", onde: { modo: "segue", rastreio: "e" } })).toBe("e");
    expect(r({ tipo: "velocidade_exibida", onde: { modo: "fixa", lugar: {} } })).toBeNull();
    expect(r({ tipo: "regua" })).toBeNull();
  });
});
