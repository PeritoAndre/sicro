/**
 * Gerador da "Seção de Metodologia — Estimativa de Distância" (fotogrametria).
 *
 * Espelha `speed-methodology.ts`: dado um `VideoDistanceMeasurement` persistido
 * + sua calibração, produz um fragmento TipTap (`JSONContent[]`) inserível no
 * laudo via `editor.chain().focus().insertContent(...)`.
 *
 * Decisões (idênticas à velocidade):
 *   - **Só texto** (heading + parágrafos com travessão "—"; sem bulletList),
 *     para render idêntico em editor / HTML / PDF / DOCX (KNOWN_LIMITATIONS §1:
 *     no DOCX a imagem é placeholder; o TEXTO é o carrier confiável).
 *   - **Honestidade pericial**: resultado como ESTIMATIVA ("a distância foi
 *     ESTIMADA em X m", nunca "a distância é X"); lista TODAS as ressalvas, o
 *     escopo da incerteza e a reprodutibilidade. A conclusão é do perito.
 *   - Função **pura** (sem I/O, sem Tauri) — determinística e testável. A
 *     "data de geração" usada é o `created_at` do registro (o anel reproduzível
 *     exato), não `Date.now()`.
 *
 * Diferença-chave para a velocidade: distância de 2 pontos NÃO tem intervalo de
 * confiança de regressão — a ÚNICA fonte de incerteza é o Monte Carlo.
 */

import type { JSONContent } from "@tiptap/core";
import type { VideoSpeedCalibration } from "@domain/video_speed";
import type {
  McSigmasDistance,
  VideoDistanceMeasurement,
} from "@domain/video_distance";

const HEADING_STYLE = "titulo_2";
const OBS_STYLE = "observacao";

// ---------------------------------------------------------------------------
// helpers (espelham speed-methodology.ts)

function p(text: string, style?: string): JSONContent {
  const attrs: Record<string, unknown> = {};
  if (style) attrs["data-laudo-style"] = style;
  return {
    type: "paragraph",
    attrs: Object.keys(attrs).length ? attrs : undefined,
    content: text ? [{ type: "text", text }] : [],
  };
}

function h(level: 1 | 2 | 3, text: string, style?: string): JSONContent {
  const attrs: Record<string, unknown> = { level };
  if (style) attrs["data-laudo-style"] = style;
  return { type: "heading", attrs, content: [{ type: "text", text }] };
}

/** Número com vírgula decimal (PT-BR). */
function num(n: number, decimals: number): string {
  return n.toFixed(decimals).replace(".", ",");
}

function methodLabel(method: string): string {
  switch (method) {
    case "plane":
      return "plano (homografia por 4 pontos coplanares)";
    case "line":
      return "linha (segmento de comprimento conhecido, 2 pontos)";
    case "cross_ratio":
      return "razão cruzada (referências colineares ao longo da via, modelo 1D)";
    default:
      return method;
  }
}

function refSourceLabel(src: string): string {
  switch (src) {
    case "campo":
      return "medida obtida em campo";
    case "norma_viaria":
      return "dimensão presumida a partir de norma viária";
    case "entre_eixos":
      return "distância entre-eixos do veículo (valor presumido)";
    default:
      return src;
  }
}

function fmtSigma(v: number, unit: string): string {
  return unit === "px" ? num(v, 1) : num(v, 3);
}

// ---------------------------------------------------------------------------
// gerador

/**
 * Produz o fragmento (heading + parágrafos) da seção de metodologia para uma
 * medição de distância. `calibration` pode ser `null` (não pôde ser
 * recuperada) — nesse caso o detalhamento da calibração vira uma nota
 * explícita, sem inventar dado.
 */
export function buildDistanceMethodologyContent(
  m: VideoDistanceMeasurement,
  calibration: VideoSpeedCalibration | null,
): JSONContent[] {
  const out: JSONContent[] = [];

  out.push(
    h(2, "DA ESTIMATIVA DE DISTÂNCIA POR ANÁLISE DE VÍDEO", HEADING_STYLE),
  );

  // --- Método ---
  out.push(
    p(
      "A distância foi estimada por exame técnico do vídeo, em modo manual: o(a) " +
        "perito(a) marcou dois pontos — extremidades A e B — sobre um quadro (frame) " +
        "extraído do vídeo original pela ferramenta ffmpeg, e a medida real foi obtida " +
        "projetando-se esses pontos do plano da imagem para o plano do mundo por meio da " +
        "calibração geométrica da cena. Por se basear em projeção sobre um plano, os " +
        "pontos devem situar-se SOBRE o plano calibrado (ex.: contato com o solo).",
    ),
  );

  if (calibration) {
    const rms =
      calibration.residuals_px != null
        ? ` O erro médio quadrático (RMS) de reprojeção dos pontos de calibração foi de ${num(
            calibration.residuals_px,
            3,
          )} m, indicando a qualidade do ajuste geométrico.`
        : " O resíduo de reprojeção não foi registrado para esta calibração.";
    out.push(
      p(
        `A correspondência imagem–mundo foi estabelecida por calibração do tipo ${methodLabel(
          calibration.method,
        )}, tendo como referência métrica ${refSourceLabel(
          calibration.reference_source,
        )}.${rms} Esta medição CONSOME a mesma calibração da cena, sem recalibração própria.`,
      ),
    );
  } else {
    out.push(
      p(
        "A calibração utilizada nesta medição não pôde ser recuperada para " +
          "detalhamento nesta seção.",
      ),
    );
  }

  // --- Resultado (ESTIMATIVA, não fato) ---
  out.push(
    p(
      `Os pontos marcados foram A (${num(m.p1_px, 0)}, ${num(m.p1_py, 0)}) e B (` +
        `${num(m.p2_px, 0)}, ${num(m.p2_py, 0)}), em pixels da imagem. Com base nesses ` +
        `pontos, a distância entre A e B foi ESTIMADA em ${num(m.distance_m, 2)} m.`,
    ),
  );

  // --- Monte Carlo (única fonte de incerteza) OU ausência explícita ---
  const hasMc = m.mc_p2_5_m != null && m.mc_p97_5_m != null;
  if (hasMc) {
    const failed = m.mc_failed ? `, ${m.mc_failed} descartadas` : "";
    const mean =
      m.mc_mean_m != null ? `, com média de ${num(m.mc_mean_m, 2)} m` : "";
    out.push(
      p(
        `A análise de incerteza por simulação de Monte Carlo (${m.mc_n ?? 0} ` +
          `iterações${failed}) resultou no intervalo de 95% (percentis 2,5 a 97,5) ` +
          `de ${num(m.mc_p2_5_m!, 2)} a ${num(m.mc_p97_5_m!, 2)} m${mean}.`,
      ),
    );
  } else {
    out.push(
      p(
        "Não foi executada simulação de Monte Carlo nesta medição (incertezas σ não " +
          "informadas pelo(a) perito(a)); portanto, o resultado limita-se à distância " +
          "pontual, SEM intervalo de incerteza. Cabe observar que uma distância entre " +
          "dois pontos não possui intervalo de confiança de regressão — a única fonte " +
          "de incerteza quantificável é a simulação de Monte Carlo.",
      ),
    );
  }

  // --- Fontes de incerteza modeladas (escopo do intervalo) ---
  if (m.mc_sigmas) {
    const s: McSigmasDistance = m.mc_sigmas;
    const modeled: string[] = [];
    const notModeled: string[] = [];
    const consider = (val: number, label: string, unit: string) => {
      if (val > 0) modeled.push(`${label} (σ = ${fmtSigma(val, unit)} ${unit})`);
      else notModeled.push(label);
    };
    consider(s.measure_px, "marcação dos dois pontos", "px");
    consider(s.calibration_px, "calibração", "px");
    consider(s.world_m, "dimensão real da referência", "m");
    if (modeled.length > 0) {
      out.push(
        p(
          `Fontes de incerteza modeladas no intervalo Monte Carlo: ${modeled.join(
            "; ",
          )}.`,
        ),
      );
    }
    if (notModeled.length > 0) {
      out.push(
        p(
          `Fontes NÃO incluídas no intervalo (σ = 0, portanto fora do escopo da ` +
            `incerteza calculada): ${notModeled.join("; ")}.`,
        ),
      );
    }
  } else {
    out.push(
      p(
        "Nenhuma fonte de incerteza foi modelada estatisticamente, pois a " +
          "simulação de Monte Carlo não foi executada.",
      ),
    );
  }

  // --- Ressalvas técnicas (lista COMPLETA, sem omitir — inclui a 1D da razão
  //     cruzada quando aplicável, pois já vem persistida em limitations). ---
  if (m.limitations.length > 0) {
    out.push(p("Ressalvas técnicas aplicáveis a esta estimativa:"));
    for (const lim of m.limitations) {
      out.push(p(`— ${lim}`, OBS_STYLE));
    }
  }

  // --- Reprodutibilidade (semente + sigmas + registro exato) ---
  if (m.mc_seed != null && m.mc_sigmas) {
    const s = m.mc_sigmas;
    out.push(
      p(
        `Reprodutibilidade: este resultado pode ser reproduzido a partir deste laudo e ` +
          `do arquivo do caso (.sicro). Semente (seed) do Monte Carlo: ${m.mc_seed}; ` +
          `sigmas utilizados — marcação ${num(s.measure_px, 1)} px, calibração ` +
          `${num(s.calibration_px, 1)} px, dimensão real ${num(s.world_m, 3)} m.`,
      ),
    );
  } else {
    out.push(
      p(
        "Reprodutibilidade: a simulação de Monte Carlo não foi executada (sem semente " +
          "registrada). A distância pontual é determinística e reproduzível a partir " +
          "dos mesmos dois pontos marcados e da mesma calibração.",
      ),
    );
  }
  out.push(
    p(
      `Registro de origem (referência reproduzível): medição id ${m.id}, registrada em ` +
        `${m.created_at}.`,
      OBS_STYLE,
    ),
  );

  // --- Enquadramento (auxílio de medição; conclusão é do perito) ---
  out.push(
    p(
      "Esta estimativa constitui AUXÍLIO DE MEDIÇÃO de natureza técnica e não afirma, " +
        "por si só, a distância exata entre os pontos. A conclusão pericial é de " +
        "responsabilidade do(a) perito(a) signatário(a), à luz do conjunto probatório.",
      OBS_STYLE,
    ),
  );

  return out;
}
