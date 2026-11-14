import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Cursor, ProjetoEdicao, Selecao } from "@domain/edicao";
import { ATRASO_MS, Autosave, NOVA_TENTATIVA_MS, TETO_MS, type EstadoSalvo } from "./autosave";
import { desfazer, historicoVazio, LIMITE_PASSOS, refazer, registrar, type Retrato } from "./historico";

/** Documento falso com uma revisão que sobe; `gravar` demora `demora` ms (relógio falso). */
function montar(demora = 0, falhas = 0) {
  let rev = 0;
  const gravados: { rev: number; quando: number }[] = [];
  const estados: EstadoSalvo[] = [];
  let falhar = falhas;
  const a = new Autosave({
    revisao: () => rev,
    salva: 0,
    gravar: () =>
      new Promise<number>((ok, erro) => {
        const r = rev;
        const fim = () => {
          if (falhar > 0) {
            falhar--;
            erro(new Error("disco"));
            return;
          }
          gravados.push({ rev: r, quando: Date.now() });
          ok(r);
        };
        // sem demora, termina na mesma volta (o relógio falso adia setTimeout(0) em 1 ms)
        if (demora > 0) setTimeout(fim, demora);
        else fim();
      }),
    estado: (e) => estados.push(e),
  });
  const mudar = () => {
    rev++;
    a.mudou();
  };
  return { a, mudar, gravados, estados, rev: () => rev };
}

describe("autosave", () => {
  beforeEach(() => {
    vi.useFakeTimers({ now: 0 });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("grava 3 s depois da ÚLTIMA mudança", async () => {
    const s = montar();
    s.mudar();
    await vi.advanceTimersByTimeAsync(2000);
    s.mudar();
    await vi.advanceTimersByTimeAsync(2999);
    expect(s.gravados).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1);
    expect(s.gravados).toEqual([{ rev: 2, quando: 2000 + ATRASO_MS }]);
    expect(s.estados.at(-1)).toBe("salvo");
  });

  it("mudança a cada 1 s por 40 s: grava pelo teto de 15 s (pelo menos 2 vezes)", async () => {
    const s = montar();
    for (let i = 0; i < 40; i++) {
      s.mudar();
      await vi.advanceTimersByTimeAsync(1000);
    }
    expect(s.gravados.length).toBeGreaterThanOrEqual(2);
    expect(s.gravados[0]?.quando).toBeLessThanOrEqual(TETO_MS);
    for (let i = 1; i < s.gravados.length; i++) {
      expect((s.gravados[i]?.quando ?? 0) - (s.gravados[i - 1]?.quando ?? 0)).toBeLessThanOrEqual(TETO_MS + 1000);
    }
    // parou de mudar: a última sai 3 s depois
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(s.gravados.at(-1)?.rev).toBe(40);
  });

  it("mudança durante a gravação: uma gravação por vez e regrava depois", async () => {
    // gravação mais longa que o prazo: o prazo da mudança nova vence no meio dela
    const s = montar(5000);
    s.mudar();
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(s.a.gravando).toBe(true);
    s.mudar();
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(s.gravados).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(2000);
    expect(s.gravados.map((g) => g.rev)).toEqual([1]);
    expect(s.a.pendente).toBe(true);
    // a gravação seguinte começa ao terminar a primeira (o prazo já passou; o relógio falso põe 1 ms)
    await vi.advanceTimersByTimeAsync(1);
    expect(s.a.gravando).toBe(true);
    await vi.advanceTimersByTimeAsync(5000);
    expect(s.gravados.map((g) => g.rev)).toEqual([1, 2]);
    expect(s.a.pendente).toBe(false);
  });

  it("relógio do sistema atrasado durante a gravação não adia a seguinte", async () => {
    vi.setSystemTime(10 * 3600_000);
    const s = montar(5000);
    s.mudar();
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    s.mudar();
    // o prazo da mudança nova vence com a gravação em curso
    await vi.advanceTimersByTimeAsync(ATRASO_MS + 500);
    // acerto do relógio: uma hora para trás
    vi.setSystemTime(Date.now() - 3600_000);
    await vi.advanceTimersByTimeAsync(1500);
    expect(s.gravados.map((g) => g.rev)).toEqual([1]);
    await vi.advanceTimersByTimeAsync(ATRASO_MS + 5000);
    expect(s.gravados.map((g) => g.rev)).toEqual([1, 2]);
  });

  it("prazos do §8.4", () => {
    expect([ATRASO_MS, TETO_MS]).toEqual([3000, 15000]);
  });

  it("descarregar espera a gravação em curso e grava o que faltou", async () => {
    const s = montar(500);
    s.mudar();
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    s.mudar();
    let pronto = false;
    void s.a.descarregar().then(() => (pronto = true));
    await vi.advanceTimersByTimeAsync(500);
    expect(pronto).toBe(false);
    await vi.advanceTimersByTimeAsync(500);
    expect(pronto).toBe(true);
    expect(s.gravados.map((g) => g.rev)).toEqual([1, 2]);
  });

  it("descarregar sem pendência não grava", async () => {
    const s = montar();
    await s.a.descarregar();
    expect(s.gravados).toHaveLength(0);
  });

  it("erro vira estado \"erro\" e tenta de novo", async () => {
    const s = montar(0, 1);
    s.mudar();
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(s.estados.at(-1)).toBe("erro");
    // mudança nova não esconde o erro
    s.mudar();
    expect(s.estados.at(-1)).toBe("erro");
    await vi.advanceTimersByTimeAsync(NOVA_TENTATIVA_MS);
    expect(s.gravados.map((g) => g.rev)).toEqual([2]);
    expect(s.estados.at(-1)).toBe("salvo");
  });

  it("descarregar rejeita quando a gravação falha", async () => {
    const s = montar(0, 5);
    s.mudar();
    await expect(s.a.descarregar()).rejects.toThrow("disco");
  });

  it("cancelar: nada mais é gravado", async () => {
    const s = montar();
    s.mudar();
    await s.a.cancelar();
    await vi.advanceTimersByTimeAsync(TETO_MS * 2);
    expect(s.gravados).toHaveLength(0);
    s.a.retomar();
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(s.gravados).toHaveLength(1);
  });
});

describe("histórico", () => {
  const doc = (n: number) => ({ titulo: `d${n}` }) as unknown as ProjetoEdicao;
  const sel = (n: number): Selecao => ({ itens: [], foco: { k: "clipe", id: `s${n}` }, efeito: null });
  const cur: Cursor = { modo: "programa", q: 0 };
  const r = (n: number): Retrato => ({ doc: doc(n), selecao: sel(n), cursor: cur });

  it("limite de 200 passos: o mais antigo sai", () => {
    expect(LIMITE_PASSOS).toBe(200);
    let h = historicoVazio();
    for (let i = 0; i < LIMITE_PASSOS + 5; i++) h = registrar(h, "x", r(i), i * 2000);
    expect(h.passado).toHaveLength(LIMITE_PASSOS);
    expect(h.passado[0]?.doc.titulo).toBe("d5");
  });

  it("agrupa a mesma chave dentro de 1 s; a janela anda com a última mudança", () => {
    let h = historicoVazio();
    h = registrar(h, "Brilho", r(0), 0, "brilho");
    h = registrar(h, "Brilho", r(1), 600, "brilho");
    h = registrar(h, "Brilho", r(2), 1400, "brilho");
    expect(h.passado).toHaveLength(1);
    expect(h.passado[0]?.doc.titulo).toBe("d0");
    h = registrar(h, "Brilho", r(3), 2500, "brilho");
    h = registrar(h, "Contraste", r(4), 2600, "contraste");
    h = registrar(h, "Nome", r(5), 2700);
    h = registrar(h, "Nome", r(6), 2800);
    expect(h.passado.map((p) => p.doc.titulo)).toEqual(["d0", "d3", "d4", "d5", "d6"]);
  });

  it("desfazer e refazer levam documento, seleção e cursor", () => {
    let h = historicoVazio();
    h = registrar(h, "A", r(0), 0);
    h = registrar(h, "B", r(1), 10);
    const d1 = desfazer(h, r(2), 20);
    expect(d1?.alvo.doc.titulo).toBe("d1");
    expect(d1?.alvo.selecao.foco?.id).toBe("s1");
    const f1 = refazer(d1!.h, r(1), 30);
    expect(f1?.alvo.doc.titulo).toBe("d2");
    expect(f1?.alvo.rotulo).toBe("B");
    expect(refazer(f1!.h, r(2), 40)).toBeNull();
  });
});
