/**
 * Extração heurística de campos a partir de texto (OCR revisado ou camada
 * textual). 100% determinística e local. Marca tudo como `heuristica` e EXIGE
 * revisão humana — jamais afirma que um campo está correto. CPF/CNPJ usam
 * validação de dígito verificador apenas para *graduar a confiança*, não para
 * validar o documento.
 */

export interface ExtractedField {
  field_name: string;
  field_value: string;
  confidence: number;
}

function onlyDigits(s: string): string {
  return s.replace(/\D/g, "");
}

/** Valida dígitos verificadores de CPF (apenas para graduar confiança). */
export function isValidCPF(raw: string): boolean {
  const d = onlyDigits(raw);
  if (d.length !== 11) return false;
  if (/^(\d)\1{10}$/.test(d)) return false; // todos iguais
  const calc = (len: number): number => {
    let sum = 0;
    for (let i = 0; i < len; i++) {
      sum += Number(d[i]) * (len + 1 - i);
    }
    const r = (sum * 10) % 11;
    return r === 10 ? 0 : r;
  };
  return calc(9) === Number(d[9]) && calc(10) === Number(d[10]);
}

/** Valida dígitos verificadores de CNPJ (apenas para graduar confiança). */
export function isValidCNPJ(raw: string): boolean {
  const d = onlyDigits(raw);
  if (d.length !== 14) return false;
  if (/^(\d)\1{13}$/.test(d)) return false;
  const calc = (len: number): number => {
    const weights =
      len === 12
        ? [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]
        : [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    let sum = 0;
    for (let i = 0; i < len; i++) {
      sum += Number(d[i]) * (weights[i] ?? 0);
    }
    const r = sum % 11;
    return r < 2 ? 0 : 11 - r;
  };
  return calc(12) === Number(d[12]) && calc(13) === Number(d[13]);
}

interface SimpleRule {
  name: string;
  re: RegExp;
  conf: number;
}

// Regras simples (sem validador). Ordem importa pouco — há dedupe ao final.
const SIMPLE_RULES: SimpleRule[] = [
  // Placa Mercosul: LLL N L NN
  { name: "Placa", re: /\b[A-Z]{3}\d[A-Z]\d{2}\b/g, conf: 0.82 },
  // Placa antiga: LLL-NNNN ou LLLNNNN
  { name: "Placa", re: /\b[A-Z]{3}-?\d{4}\b/g, conf: 0.7 },
  // Chassi (VIN): 17 alfanuméricos sem I/O/Q
  { name: "Chassi", re: /\b[A-HJ-NPR-Z0-9]{17}\b/g, conf: 0.58 },
  // Processo CNJ: NNNNNNN-DD.AAAA.J.TR.OOOO
  {
    name: "Processo",
    re: /\b\d{7}-\d{2}\.\d{4}\.\d\.\d{2}\.\d{4}\b/g,
    conf: 0.88,
  },
  // Datas
  { name: "Data", re: /\b\d{2}\/\d{2}\/\d{4}\b/g, conf: 0.72 },
  { name: "Data", re: /\b\d{2}-\d{2}-\d{4}\b/g, conf: 0.68 },
  { name: "Data", re: /\b\d{4}-\d{2}-\d{2}\b/g, conf: 0.66 },
  // Hora
  { name: "Hora", re: /\b\d{2}:\d{2}(?::\d{2})?\b/g, conf: 0.55 },
  // Valor monetário
  { name: "Valor", re: /R\$\s?\d{1,3}(?:\.\d{3})*,\d{2}/g, conf: 0.8 },
  // E-mail
  { name: "E-mail", re: /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g, conf: 0.72 },
  // Telefone BR
  { name: "Telefone", re: /\(?\d{2}\)?\s?9?\d{4}-?\d{4}\b/g, conf: 0.5 },
  // CEP
  { name: "CEP", re: /\b\d{5}-\d{3}\b/g, conf: 0.62 },
];

function pushUnique(
  out: ExtractedField[],
  seen: Set<string>,
  field: ExtractedField,
): void {
  const key = `${field.field_name}::${field.field_value.toUpperCase()}`;
  if (seen.has(key)) return;
  seen.add(key);
  out.push(field);
}

/**
 * Extrai campos candidatos do texto. Todos vêm como `heuristica` e devem ser
 * revisados. Não há garantia de completude nem de exatidão.
 */
export function extractFields(text: string): ExtractedField[] {
  const out: ExtractedField[] = [];
  const seen = new Set<string>();
  if (!text || !text.trim()) return out;

  // CPF (com validação de DV para graduar confiança)
  for (const m of text.matchAll(/\d{3}\.?\d{3}\.?\d{3}-?\d{2}/g)) {
    const value = m[0];
    const valid = isValidCPF(value);
    pushUnique(out, seen, {
      field_name: "CPF",
      field_value: value,
      confidence: valid ? 0.92 : 0.5,
    });
  }
  // CNPJ
  for (const m of text.matchAll(/\d{2}\.?\d{3}\.?\d{3}\/?\d{4}-?\d{2}/g)) {
    const value = m[0];
    const valid = isValidCNPJ(value);
    pushUnique(out, seen, {
      field_name: "CNPJ",
      field_value: value,
      confidence: valid ? 0.92 : 0.5,
    });
  }

  for (const rule of SIMPLE_RULES) {
    for (const m of text.matchAll(rule.re)) {
      const value = m[0].trim();
      if (!value) continue;
      pushUnique(out, seen, {
        field_name: rule.name,
        field_value: value,
        confidence: rule.conf,
      });
    }
  }

  return out;
}
