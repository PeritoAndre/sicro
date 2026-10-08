/**
 * Pessoa articulada do croqui: esqueleto 2D (vista de cima) com cinemática de dois ossos.
 * Posição em px de mundo; corpo em metros pela altura; pose em fração da altura, no
 * referencial do corpo (pelve na origem, cabeça para −y). Sem React/Konva/canvas.
 */

import type { PersonPosicao, SicroPersonObject, SicroPoint } from "./schema";

export interface Vec {
  x: number;
  y: number;
}

const PI = Math.PI;
const clamp = (v: number, a: number, b: number) => Math.max(a, Math.min(b, v));
export const vrot = (p: Vec, a: number): Vec => ({ x: p.x * Math.cos(a) - p.y * Math.sin(a), y: p.x * Math.sin(a) + p.y * Math.cos(a) });
export const vadd = (a: Vec, b: Vec): Vec => ({ x: a.x + b.x, y: a.y + b.y });
export const vsub = (a: Vec, b: Vec): Vec => ({ x: a.x - b.x, y: a.y - b.y });
export const vmul = (a: Vec, k: number): Vec => ({ x: a.x * k, y: a.y * k });

export const PERSON_POSICOES: [PersonPosicao, string][] = [
  ["dorsal", "Dorsal"],
  ["ventral", "Ventral"],
  ["lat_e", "Lateral esq."],
  ["lat_d", "Lateral dir."],
  ["empe", "Em pé"],
];

export const PERSON_COMP: Record<SicroPersonObject["comp"], number> = { magro: 0.86, medio: 1, robusto: 1.2 };

/** Segmentos em fração da altura (Drillis & Contini). */
export const PERSON_SEG = { braco: 0.186, antebraco: 0.146, mao: 0.108, coxa: 0.245, perna: 0.246 };

export const personLateral = (o: Pick<SicroPersonObject, "posicao">) => o.posicao === "lat_e" || o.posicao === "lat_d";
/** Lado da frente do corpo em x local (só faz sentido de lado). */
export const personFrente = (o: Pick<SicroPersonObject, "posicao">) => (o.posicao === "lat_d" ? 1 : -1);

const rad = (deg: number) => (deg * PI) / 180;

/** Local (m) → mundo (px). */
export function personToWorld(o: SicroPersonObject, p: Vec, pxPerM: number): Vec {
  return vadd({ x: o.x, y: o.y }, vmul(vrot(p, rad(o.rotation)), pxPerM));
}
/** Mundo (px) → local (m). */
export function personToLocal(o: SicroPersonObject, p: Vec, pxPerM: number): Vec {
  return vrot(vmul(vsub(p, { x: o.x, y: o.y }), 1 / pxPerM), -rad(o.rotation));
}

function ik(R: Vec, T: Vec, a: number, b: number, s: number): { E: Vec; T: Vec } {
  const dx = T.x - R.x;
  const dy = T.y - R.y;
  const dd = clamp(Math.hypot(dx, dy), Math.abs(a - b) + 1e-4, a + b - 1e-4);
  const th = Math.atan2(dy, dx);
  const al = Math.acos(clamp((a * a + dd * dd - b * b) / (2 * a * dd), -1, 1));
  return {
    E: { x: R.x + a * Math.cos(th + s * al), y: R.y + a * Math.sin(th + s * al) },
    T: { x: R.x + dd * Math.cos(th), y: R.y + dd * Math.sin(th) },
  };
}

export interface PersonLimb {
  S: Vec;
  E: Vec;
  T: Vec;
}

export interface PersonRig {
  H: number;
  k: number;
  lat: boolean;
  /** Frente do corpo (±1 em x local), de lado. */
  m: number;
  fem: boolean;
  chest: (p: Vec) => Vec;
  torso: Vec[];
  neck: Vec;
  headC: Vec;
  hdir: Vec;
  up: Vec;
  front: Vec | null;
  arms: PersonLimb[];
  legs: PersonLimb[];
}

/** Esqueleto em metros, no referencial do corpo. */
export function personRig(o: SicroPersonObject): PersonRig {
  const H = o.altura_m;
  const k = PERSON_COMP[o.comp] ?? 1;
  const fem = o.perfil === "F";
  const sh = fem ? 0.9 : 1;
  const hp = fem ? 1.08 : 1;
  const lat = personLateral(o);
  const m = personFrente(o);
  const bend = rad(o.curva);
  const hinge = { x: 0, y: -0.13 * H };
  const chest = (p: Vec) => vadd(hinge, vrot(vsub(p, hinge), bend));
  let torso: Vec[];
  if (!lat) {
    const wf = fem ? 0.88 : 1;
    const left: [number, number, number, number][] = [
      [-0.02, 0.066, 0, 0], [-0.075, 0.062, 0, 1], [-0.112 * hp, 0.02, 0, 1], [-0.114 * hp, -0.035, 0, 1], [-0.094 * wf, -0.115, 0, 1],
      [-0.1, -0.17, 1, 1], [-0.108 * sh, -0.222, 1, 1], [-0.114 * sh, -0.25, 1, 1], [-0.13 * sh, -0.272, 1, 1], [-0.118 * sh, -0.297, 1, 1],
      [-0.074, -0.312, 1, 1], [-0.034, -0.328, 1, 0],
    ];
    const right = left.slice().reverse().map(([x, y, c, s]) => [-x, y, c, s] as [number, number, number, number]);
    torso = left.concat(right).map(([x, y, c, sc]) => {
      const p = { x: x * H * (sc ? k : 1), y: y * H };
      return c ? chest(p) : p;
    });
  } else {
    // Perfil: barriga e peito na frente (m), costas e glúteo atrás.
    const fr: [number, number, number][] = [[0.03, 0.062, 0], [0.07, 0.02, 0], [0.072, -0.06, 0], [0.066, -0.13, 0], [0.08 * (fem ? 1.12 : 1), -0.205, 1], [0.074, -0.25, 1], [0.05, -0.293, 1], [0.03, -0.33, 1]];
    const bk: [number, number, number][] = [[-0.035, -0.325, 1], [-0.068, -0.29, 1], [-0.076, -0.24, 1], [-0.062, -0.17, 1], [-0.05, -0.11, 0], [-0.084 * hp, -0.03, 0], [-0.07, 0.04, 0], [-0.035, 0.066, 0]];
    torso = fr.concat(bk).map(([x, y, c]) => {
      const p = { x: x * m * H * k, y: y * H };
      return c ? chest(p) : p;
    });
  }
  const neck = chest({ x: 0, y: -0.31 * H });
  const up = vrot({ x: 0, y: -1 }, bend);
  const hdir = vrot(up, rad(o.cabeca));
  const headC = vadd(neck, vmul(hdir, 0.1 * H));
  const front = lat ? vrot({ x: m, y: 0 }, bend + rad(o.cabeca)) : null;
  const shoulders = lat
    ? [chest({ x: -0.012 * m * H, y: -0.285 * H }), chest({ x: 0.012 * m * H, y: -0.285 * H })]
    : [chest({ x: -0.104 * H * sh * k, y: -0.278 * H }), chest({ x: 0.104 * H * sh * k, y: -0.278 * H })];
  const hips = lat
    ? [{ x: -0.01 * m * H, y: 0.005 * H }, { x: 0.01 * m * H, y: 0.005 * H }]
    : [{ x: -0.068 * H * hp * k, y: 0.03 * H }, { x: 0.068 * H * hp * k, y: 0.03 * H }];
  const arms = [0, 1].map((i) => {
    const r = ik(shoulders[i]!, vmul(o.pose.wrist[i]!, H), PERSON_SEG.braco * H, PERSON_SEG.antebraco * H, o.pose.sArm[i]!);
    return { S: shoulders[i]!, E: r.E, T: r.T };
  });
  const legs = [0, 1].map((i) => {
    const r = ik(hips[i]!, vmul(o.pose.ankle[i]!, H), PERSON_SEG.coxa * H, PERSON_SEG.perna * H, o.pose.sLeg[i]!);
    return { S: hips[i]!, E: r.E, T: r.T };
  });
  return { H, k, lat, m, fem, chest, torso, neck, headC, hdir, up, front, arms, legs };
}

// ---------------------------------------------------------------------------
// Poses (fração da altura). "lat": versão de lado, x positivo = frente do corpo.

interface PoseDef {
  wrist: [number, number][];
  ankle: [number, number][];
  curva: number;
  cabeca: number;
}

export const PERSON_POSES: Record<string, { nome: string; frente: PoseDef; lat: PoseDef }> = {
  estendido: {
    nome: "Estendido",
    frente: { wrist: [[-0.13, 0.02], [0.13, 0.02]], ankle: [[-0.07, 0.5], [0.07, 0.5]], curva: 0, cabeca: 0 },
    lat: { wrist: [[0.04, 0.03], [0.08, 0]], ankle: [[-0.01, 0.5], [0.05, 0.49]], curva: 0, cabeca: 0 },
  },
  abertos: {
    nome: "Braços abertos",
    frente: { wrist: [[-0.42, -0.36], [0.42, -0.36]], ankle: [[-0.17, 0.47], [0.17, 0.47]], curva: 0, cabeca: 0 },
    lat: { wrist: [[0.3, -0.36], [0.33, -0.24]], ankle: [[-0.02, 0.5], [0.14, 0.46]], curva: 0, cabeca: 0 },
  },
  acima: {
    nome: "Braços acima",
    frente: { wrist: [[-0.16, -0.6], [0.14, -0.62]], ankle: [[-0.09, 0.5], [0.06, 0.5]], curva: 0, cabeca: 0 },
    lat: { wrist: [[0.06, -0.62], [0.14, -0.58]], ankle: [[-0.02, 0.5], [0.05, 0.49]], curva: 0, cabeca: 0 },
  },
  recolhido: {
    nome: "Recolhido",
    frente: { wrist: [[-0.12, -0.2], [0.12, -0.2]], ankle: [[-0.13, 0.22], [0.13, 0.22]], curva: 0, cabeca: 0 },
    lat: { wrist: [[0.15, -0.36], [0.18, -0.28]], ankle: [[0.1, 0.25], [0.17, 0.21]], curva: 22, cabeca: 14 },
  },
  seguranca: {
    nome: "Lateral de segurança",
    frente: { wrist: [[-0.3, -0.5], [0.24, 0.08]], ankle: [[-0.05, 0.5], [0.27, 0.28]], curva: 6, cabeca: -18 },
    lat: { wrist: [[0.16, -0.4], [-0.04, 0.06]], ankle: [[0, 0.5], [0.27, 0.26]], curva: 8, cabeca: 8 },
  },
};

type PoseFields = Pick<SicroPersonObject, "pose" | "curva" | "cabeca">;

/** Cotovelos e joelhos para o lado natural: de frente, para fora; de lado, cotovelo para baixo e joelho para a frente. */
export function personAutoBend(o: SicroPersonObject): SicroPersonObject["pose"] {
  const lat = personLateral(o);
  const m = personFrente(o);
  const pose = { ...o.pose, sArm: [...o.pose.sArm] as [number, number], sLeg: [...o.pose.sLeg] as [number, number] };
  for (const key of ["arms", "legs"] as const) {
    const list = key === "arms" ? pose.sArm : pose.sLeg;
    for (let i = 0; i < 2; i++) {
      let best = 1;
      let bestScore = -Infinity;
      for (const sg of [1, -1]) {
        list[i] = sg;
        const E = personRig({ ...o, pose })[key][i]!.E;
        const score = lat ? (key === "arms" ? E.y + E.x * m * 0.4 : E.x * m) : i === 0 ? -E.x : E.x;
        if (score > bestScore) {
          bestScore = score;
          best = sg;
        }
      }
      list[i] = best;
    }
  }
  return pose;
}

export function personApplyPose(o: SicroPersonObject, key: string): PoseFields {
  const def = PERSON_POSES[key] ?? PERSON_POSES.estendido!;
  const lat = personLateral(o);
  const m = personFrente(o);
  const p = lat ? def.lat : def.frente;
  const fx = (x: number) => (lat ? x * m : x);
  const draft: SicroPersonObject = {
    ...o,
    pose: {
      ...o.pose,
      wrist: p.wrist.map(([x, y]) => ({ x: fx(x), y })) as [Vec, Vec],
      ankle: p.ankle.map(([x, y]) => ({ x: fx(x), y })) as [Vec, Vec],
    },
    curva: lat ? p.curva * m : p.curva,
    cabeca: lat ? p.cabeca * m : p.cabeca,
  };
  return { pose: personAutoBend(draft), curva: draft.curva, cabeca: draft.cabeca };
}

/**
 * Troca de posição. Entre as duas laterais a frente inverte, então a pose é espelhada
 * (sem isso, braços e pernas ficavam do lado de trás do corpo); de frente ↔ de lado, pose padrão.
 */
export function personChangePosicao(o: SicroPersonObject, posicao: PersonPosicao): Partial<SicroPersonObject> {
  if (posicao === o.posicao) return {};
  const next: SicroPersonObject = { ...o, posicao };
  if (posicao === "empe" || o.posicao === "empe") {
    return posicao === "empe" ? { posicao } : { posicao, ...personApplyPose(next, personLateral(next) ? "recolhido" : "estendido") };
  }
  const wasLat = personLateral(o);
  const isLat = personLateral(next);
  if (wasLat && isLat) {
    const mirrored: SicroPersonObject = {
      ...next,
      pose: {
        wrist: o.pose.wrist.map((p) => ({ x: -p.x, y: p.y })) as [Vec, Vec],
        ankle: o.pose.ankle.map((p) => ({ x: -p.x, y: p.y })) as [Vec, Vec],
        sArm: [-o.pose.sArm[0], -o.pose.sArm[1]] as [number, number],
        sLeg: [-o.pose.sLeg[0], -o.pose.sLeg[1]] as [number, number],
      },
      curva: -o.curva,
      cabeca: -o.cabeca,
    };
    return { posicao, pose: mirrored.pose, curva: mirrored.curva, cabeca: mirrored.cabeca };
  }
  if (wasLat !== isLat) return { posicao, ...personApplyPose(next, isLat ? "recolhido" : "estendido") };
  return { posicao, pose: personAutoBend(next) };
}

/** Pessoa nova em `p` (px), cabeça para o topo da folha; sem rótulo. */
export function makePerson(posicao: PersonPosicao, p: SicroPoint): SicroPersonObject {
  const base: SicroPersonObject = {
    id: `person_${typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`}`,
    layer_id: "layer_objects",
    kind: "person",
    posicao,
    x: p.x,
    y: p.y,
    rotation: 0,
    altura_m: 1.75,
    comp: "medio",
    perfil: "M",
    curva: 0,
    cabeca: 0,
    acab: "normal",
    cor: "branco",
    pose: { wrist: [{ x: -0.13, y: 0.02 }, { x: 0.13, y: 0.02 }], ankle: [{ x: -0.07, y: 0.5 }, { x: 0.07, y: 0.5 }], sArm: [1, -1], sLeg: [-1, 1] },
    label: "",
    visible: true,
    locked: false,
    category: "pessoas",
  };
  if (posicao === "empe") return base;
  return { ...base, ...personApplyPose(base, personLateral(base) ? "recolhido" : "estendido") };
}

/** Pontos de controle em metros locais (para limites e seleção). */
function keyPoints(o: SicroPersonObject): Vec[] {
  const H = o.altura_m;
  if (o.posicao === "empe") return [{ x: -0.16 * H, y: -0.17 * H }, { x: 0.16 * H, y: 0.08 * H }];
  const R = personRig(o);
  return [R.headC, vadd(R.headC, vmul(R.hdir, 0.07 * H)), ...R.torso, ...R.arms.flatMap((a) => [a.E, a.T]), ...R.legs.flatMap((l) => [l.E, l.T])];
}

/** AABB em px de mundo. */
export function personBoundsPx(o: SicroPersonObject, pxPerM: number): { x: number; y: number; width: number; height: number } {
  const pts = keyPoints(o).map((p) => personToWorld(o, p, pxPerM));
  const pad = 0.12 * o.altura_m * pxPerM;
  const xs = pts.map((p) => p.x);
  const ys = pts.map((p) => p.y);
  const x0 = Math.min(...xs) - pad;
  const y0 = Math.min(...ys) - pad;
  return { x: x0, y: y0, width: Math.max(...xs) + pad - x0, height: Math.max(...ys) + pad - y0 };
}

const fmt = (v: number, d = 1) => v.toFixed(d).replace(".", ",");

export function personReadouts(o: SicroPersonObject): [string, string][] {
  const dir = ((Math.round(o.rotation) % 360) + 360) % 360;
  const rows: [string, string][] = [
    ["Altura", `${fmt(o.altura_m, 2)} m`],
    [o.posicao === "empe" ? "Voltada para" : "Cabeça para", `${dir}° (topo da folha = 0°)`],
  ];
  if (o.posicao !== "empe") {
    const R = personRig(o);
    const top = vadd(R.headC, vmul(R.hdir, 0.068 * o.altura_m));
    const far = Math.max(...[...R.legs, ...R.arms].map((l) => Math.hypot(l.T.x - top.x, l.T.y - top.y)));
    rows.push(["Cabeça até o ponto mais distante", `${fmt(far, 2)} m`]);
  }
  return rows;
}
