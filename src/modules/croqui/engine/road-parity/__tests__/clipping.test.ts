/**
 * Python Parity Engine — testes de clipping.
 *
 * Foco: comportamento correto E garantia de fallback (nunca quebrar
 * o croqui, mesmo com geometria degenerada).
 */

import { describe, expect, it } from "vitest";
import { clipPolylineAgainstPolygons } from "../clipping";
import type { Vec2World } from "../geometry";

const square = (cx: number, cy: number, s: number): Vec2World[] => [
  { x: cx - s, y: cy - s },
  { x: cx + s, y: cy - s },
  { x: cx + s, y: cy + s },
  { x: cx - s, y: cy + s },
];

describe("road-parity / clipPolylineAgainstPolygons", () => {
  it("sem obstáculos retorna polilinha intacta (sem passar por densificação)", () => {
    const line: Vec2World[] = [
      { x: 0, y: 0 },
      { x: 10, y: 0 },
    ];
    const { segments, report } = clipPolylineAgainstPolygons(line, []);
    // Quando não há obstáculos, retorna direto sem densificar.
    expect(segments).toHaveLength(1);
    expect(segments[0]).toEqual(line);
    expect(report.fallback_used).toBe(false);
    expect(report.segments_count).toBe(1);
  });

  it("polilinha vazia retorna [] sem erro", () => {
    const { segments, report } = clipPolylineAgainstPolygons([], []);
    expect(segments).toEqual([]);
    expect(report.fallback_used).toBe(false);
  });

  it("polilinha com 1 ponto retorna [] (sem segmento desenhável)", () => {
    const { segments } = clipPolylineAgainstPolygons([{ x: 0, y: 0 }], []);
    expect(segments).toEqual([]);
  });

  it("polilinha atravessando um quadrado é cortada nos pontos exatos", () => {
    const line: Vec2World[] = [
      { x: -20, y: 0 },
      { x: 20, y: 0 },
    ];
    const obs = [square(0, 0, 5)];
    const { segments } = clipPolylineAgainstPolygons(line, obs);
    // Após densificação interna (1m) o algoritmo gera 2 sub-polilinhas:
    //   seg1: [-20, ..., -5] (último ponto é o crossing exato)
    //   seg2: [5, ..., 20]   (primeiro ponto é o crossing exato)
    expect(segments).toHaveLength(2);
    const seg1 = segments[0]!;
    const seg2 = segments[1]!;
    expect(seg1[0]).toEqual({ x: -20, y: 0 });
    // Crossing à esquerda: último ponto do primeiro segmento.
    expect(seg1[seg1.length - 1]!.x).toBeCloseTo(-5, 3);
    // Crossing à direita: primeiro ponto do segundo segmento.
    expect(seg2[0]!.x).toBeCloseTo(5, 3);
    expect(seg2[seg2.length - 1]).toEqual({ x: 20, y: 0 });
  });

  it("polilinha totalmente dentro de um obstáculo → sem segmentos", () => {
    const line: Vec2World[] = [
      { x: 1, y: 1 },
      { x: 2, y: 2 },
    ];
    const { segments } = clipPolylineAgainstPolygons(line, [square(0, 0, 5)]);
    expect(segments).toHaveLength(0);
  });

  it("polilinha totalmente fora de um obstáculo → trecho único", () => {
    const line: Vec2World[] = [
      { x: 100, y: 100 },
      { x: 200, y: 200 },
    ];
    const { segments } = clipPolylineAgainstPolygons(line, [square(0, 0, 5)]);
    // 1 segmento. Após densificação interna ele tem ~142 pontos,
    // mas começa em (100, 100) e termina em (200, 200).
    expect(segments).toHaveLength(1);
    const seg = segments[0]!;
    expect(seg[0]).toEqual({ x: 100, y: 100 });
    expect(seg[seg.length - 1]).toEqual({ x: 200, y: 200 });
  });

  it("múltiplos obstáculos cortam a polilinha em vários trechos", () => {
    const line: Vec2World[] = [
      { x: -50, y: 0 },
      { x: 50, y: 0 },
    ];
    const obs = [square(-20, 0, 5), square(20, 0, 5)];
    const { segments } = clipPolylineAgainstPolygons(line, obs);
    expect(segments).toHaveLength(3);
  });

  it("report.obstacles_count reflete entrada", () => {
    const { report } = clipPolylineAgainstPolygons(
      [
        { x: 0, y: 0 },
        { x: 1, y: 1 },
      ],
      [square(50, 50, 1), square(60, 60, 1)],
    );
    expect(report.obstacles_count).toBe(2);
  });

  it("fallback nunca propaga erro mesmo com inputs malformados", () => {
    // Polilinha com NaN — pontoInPolygon vai produzir comparações
    // estranhas mas não deve lançar. O resultado pode ser estranho,
    // mas o teste apenas garante que NÃO há throw.
    const lineWithNaN: Vec2World[] = [
      { x: NaN, y: 0 },
      { x: 10, y: 0 },
    ];
    expect(() =>
      clipPolylineAgainstPolygons(lineWithNaN, [square(0, 0, 5)]),
    ).not.toThrow();
  });

  it("clipping NUNCA gera segmento de 1 ponto", () => {
    const line: Vec2World[] = [
      { x: -20, y: 0 },
      { x: -15, y: 0 },
      { x: -10, y: 0 },
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 15, y: 0 },
      { x: 20, y: 0 },
    ];
    const { segments } = clipPolylineAgainstPolygons(line, [
      square(0, 0, 5),
    ]);
    for (const seg of segments) {
      expect(seg.length).toBeGreaterThanOrEqual(2);
    }
  });
});
