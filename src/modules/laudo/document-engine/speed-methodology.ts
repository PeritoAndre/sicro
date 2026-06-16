/**
 * Gerador da "Seção de Metodologia — Estimativa de Velocidade".
 *
 * Dado um `VideoSpeedCalculation` persistido + sua calibração, produz um
 * fragmento de documento TipTap (`JSONContent[]`) pronto para ser inserido
 * no laudo via `editor.chain().focus().insertContent(...)` — o mesmo caminho
 * dos blocos reutilizáveis (`blocks/catalog.ts`).
 *
 * Decisões de projeto:
 *   - **Só texto** (heading + parágrafos). A exportação DOCX com imagem é
 *     placeholder (KNOWN_LIMITATIONS §1) e o renderer HTML não trata
 *     `bulletList`; logo o conteúdo é carregado por parágrafos, garantindo
 *     render idêntico no editor / HTML / PDF / DOCX.
 *   - **Honestidade pericial**: o resultado é apresentado como ESTIMATIVA
 *     (auxílio de medição), nunca como fato ("o veículo estava a X"). Lista
 *     TODAS as ressalvas, o escopo da incerteza e os dados de
 *     reprodutibilidade (semente + sigmas). A conclusão é do perito.
 *   - Função **pura** (sem I/O, sem Tauri) — testável e determinística.
 */

import type { JSONContent } from "@tiptap/core";
import type {
  McSigmas,
  VideoSpeedCalculation,
  VideoSpeedCalibration,
} from "@domain/video_speed";

const HEADING_STYLE = "titulo_2";
const OBS_STYLE = "observacao";

// ---------------------------------------------------------------------------
// helpers de construção (espelham blocks/catalog.ts)

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
 * Produz o fragmento (heading + parágrafos) da seção de metodologia para um
 * cálculo de velocidade. `calibration` pode ser `null` (a calibração não pôde
 * ser recuperada) — nesse caso o detalhamento da calibração é substituído por
 * uma nota explícita.
 */
export function buildSpeedMethodologyContent(
  calc: VideoSpeedCalculation,
  calibration: VideoSpeedCalibration | null,
): JSONContent[] {
  const out: JSONContent[] = [];

  out.push(
    h(2, "DA ESTIMATIVA DE VELOCIDADE POR ANÁLISE DE VÍDEO", HEADING_STYLE),
  );

  // --- Método ---
  out.push(
    p(
      "A velocidade foi estimada por exame técnico do vídeo, em modo manual: o(a) " +
        "perito(a) marcou a posição do veículo sobre quadros (frames) extraídos do " +
        "vídeo original pela ferramenta ffmpeg, preservando a correspondência exata " +
        "entre a imagem analisada e o instante de tempo de cada quadro.",
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
        )}.${rms}`,
      ),
    );
  } else {
    out.push(
      p(
        "A calibração utilizada neste cálculo não pôde ser recuperada para " +
          "detalhamento nesta seção.",
      ),
    );
  }

  // --- Frames / instantes ---
  const instants = calc.points
    .map((pt) => `${num(pt.actual_timestamp_s, 3)} s`)
    .join("; ");
  out.push(
    p(
      `Foram utilizados ${calc.points.length} ponto(s) de trajetória, marcados sobre ` +
        `os seguintes instantes do vídeo: ${instants}.`,
    ),
  );

  // --- Resultado (apresentado como ESTIMATIVA, não como fato) ---
  const ms = calc.velocity_kmh / 3.6;
  let resultLine =
    `Com base nos pontos marcados, a velocidade do veículo foi ESTIMADA em ` +
    `${num(calc.velocity_kmh, 1)} km/h (${num(ms, 2)} m/s).`;
  if (calc.ci_low != null && calc.ci_high != null) {
    const conf = Math.round((calc.confidence ?? 0.95) * 100);
    const r2 =
      calc.r_squared != null ? `, com R² = ${num(calc.r_squared, 4)}` : "";
    resultLine +=
      ` O intervalo de confiança de ${conf}% do ajuste por regressão é de ` +
      `${num(calc.ci_low, 1)} a ${num(calc.ci_high, 1)} km/h${r2}.`;
  } else {
    resultLine +=
      " Por se basear em apenas dois pontos, não há intervalo de confiança " +
      "estatístico: o valor corresponde à velocidade média entre os dois " +
      "instantes marcados.";
  }
  out.push(p(resultLine));

  // --- Monte Carlo (ou ausência explícita) ---
  if (calc.mc_p2_5_kmh != null && calc.mc_p97_5_kmh != null) {
    const failed = calc.mc_failed
      ? `, ${calc.mc_failed} descartadas`
      : "";
    const mean =
      calc.mc_mean_kmh != null
        ? `, com média de ${num(calc.mc_mean_kmh, 1)} km/h`
        : "";
    out.push(
      p(
        `A análise de incerteza por simulação de Monte Carlo (${calc.mc_n ?? 0} ` +
          `iterações${failed}) resultou no intervalo de 95% (percentis 2,5 a 97,5) ` +
          `de ${num(calc.mc_p2_5_kmh, 1)} a ${num(calc.mc_p97_5_kmh, 1)} km/h${mean}.`,
      ),
    );
  } else {
    out.push(
      p(
        "Não foi executada simulação de Monte Carlo neste cálculo; portanto, a " +
          "incerteza informada limita-se ao intervalo de confiança do ajuste por " +
          "regressão.",
      ),
    );
  }

  // --- Fontes de incerteza modeladas (escopo do intervalo) ---
  if (calc.mc_sigmas) {
    const s: McSigmas = calc.mc_sigmas;
    const modeled: string[] = [];
    const notModeled: string[] = [];
    const consider = (val: number, label: string, unit: string) => {
      if (val > 0) modeled.push(`${label} (σ = ${fmtSigma(val, unit)} ${unit})`);
      else notModeled.push(label);
    };
    consider(s.trajectory_px, "marcação dos pontos", "px");
    consider(s.calibration_px, "calibração", "px");
    consider(s.time_s, "tempo", "s");
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

  // --- Ressalvas técnicas (lista COMPLETA, sem omitir nenhuma) ---
  if (calc.limitations.length > 0) {
    out.push(p("Ressalvas técnicas aplicáveis a esta estimativa:"));
    for (const lim of calc.limitations) {
      out.push(p(`— ${lim}`, OBS_STYLE));
    }
  }

  // --- Reprodutibilidade ---
  if (calc.mc_seed != null && calc.mc_sigmas) {
    const s = calc.mc_sigmas;
    out.push(
      p(
        `Reprodutibilidade: este resultado pode ser reproduzido a partir deste laudo ` +
          `e do arquivo do caso (.sicro). Semente (seed) do Monte Carlo: ${calc.mc_seed}; ` +
          `sigmas utilizados — marcação ${num(s.trajectory_px, 1)} px, calibração ` +
          `${num(s.calibration_px, 1)} px, tempo ${num(s.time_s, 3)} s, dimensão real ` +
          `${num(s.world_m, 3)} m.`,
      ),
    );
  } else {
    out.push(
      p(
        "Reprodutibilidade: a simulação de Monte Carlo não foi executada (sem semente " +
          "registrada). O intervalo do ajuste por regressão é determinístico e " +
          "reproduzível a partir dos mesmos pontos marcados.",
      ),
    );
  }

  // --- Enquadramento (auxílio de medição; conclusão é do perito) ---
  out.push(
    p(
      "Esta estimativa constitui AUXÍLIO DE MEDIÇÃO de natureza técnica e não afirma, " +
        "por si só, a velocidade exata do veículo. A conclusão pericial sobre a " +
        "dinâmica do evento é de responsabilidade do(a) perito(a) signatário(a), à luz " +
        "do conjunto probatório.",
      OBS_STYLE,
    ),
  );

  return out;
}
