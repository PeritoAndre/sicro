/** Lógica pura da tela Início: título do caso, saudação, filtro/ordenação e backup. */

const DIAS = ["Domingo", "Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado"];
const MESES = [
  "janeiro", "fevereiro", "março", "abril", "maio", "junho",
  "julho", "agosto", "setembro", "outubro", "novembro", "dezembro",
];

const pad = (n: number) => String(n).padStart(2, "0");

function dmy(d: Date): string {
  return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()}`;
}

export interface DadosTitulo {
  titulo?: string | null;
  /** Nº do laudo. */
  protocolo?: string | null;
  numero_bo?: string | null;
  created_at?: string | null;
}

/** "Laudo 63404/26 · Km 09"; sem número, o nome; sem nada, "Caso de dd/mm/aaaa". */
export function tituloDoCaso(d: DadosTitulo): string {
  const titulo = d.titulo?.trim() ?? "";
  const laudo = d.protocolo?.trim() ?? "";
  const bo = d.numero_bo?.trim() ?? "";
  const numero = laudo || bo;
  const prefixo = laudo ? `Laudo ${laudo}` : bo ? `Ocorrência ${bo}` : "";
  if (titulo && prefixo) {
    // O nome já costuma trazer o número ("Laudo 63404/26, Km 09…").
    return titulo.toLowerCase().includes(numero.toLowerCase()) ? titulo : `${prefixo} · ${titulo}`;
  }
  if (titulo) return titulo;
  if (prefixo) return prefixo;
  return tituloPorData(d.created_at);
}

export function tituloPorData(createdAt?: string | null): string {
  const t = createdAt ? new Date(createdAt) : null;
  return t && !Number.isNaN(t.getTime()) ? `Caso de ${dmy(t)}` : "Caso sem nome";
}

export function temNumero(d: DadosTitulo): boolean {
  return !!(d.protocolo?.trim() || d.numero_bo?.trim());
}

/** Linha miúda do cartão: data de criação, ou aviso de que não há número. */
export function linhaMiuda(titulo: string, createdAt?: string | null): string {
  const data = tituloPorData(createdAt);
  return titulo === data ? "sem número de laudo" : data;
}

export function saudacao(hora: number, nomeCompleto?: string | null): string {
  const s = hora >= 5 && hora < 12 ? "Bom dia" : hora >= 12 && hora < 18 ? "Boa tarde" : "Boa noite";
  const primeiro = nomeCompleto?.trim().split(/\s+/)[0];
  return primeiro ? `${s}, ${primeiro}` : s;
}

/** "Quinta, 13 de novembro de 2026". */
export function dataPorExtenso(d: Date): string {
  return `${DIAS[d.getDay()]}, ${d.getDate()} de ${MESES[d.getMonth()]} de ${d.getFullYear()}`;
}

// ---------------------------------------------------------------------------

export type FiltroCasos = "todos" | "andamento" | "concluidos";

export interface CasoLista {
  workspace_id: string;
  status: string;
  last_opened_at: string;
}

export function estaConcluido(status: string): boolean {
  return status === "concluida" || status === "arquivada";
}

export function contarEmAndamento(casos: CasoLista[]): number {
  return casos.filter((c) => !estaConcluido(c.status)).length;
}

/** Fixados no topo; dentro de cada grupo, o aberto mais recentemente primeiro. */
export function filtrarOrdenar<T extends CasoLista>(
  casos: T[],
  filtro: FiltroCasos,
  fixados: ReadonlySet<string>,
): T[] {
  const ts = (c: T) => {
    const t = new Date(c.last_opened_at).getTime();
    return Number.isNaN(t) ? 0 : t;
  };
  return casos
    .filter((c) =>
      filtro === "todos" ? true : filtro === "concluidos" ? estaConcluido(c.status) : !estaConcluido(c.status),
    )
    .sort((a, b) => {
      const pa = fixados.has(a.workspace_id) ? 1 : 0;
      const pb = fixados.has(b.workspace_id) ? 1 : 0;
      return pb - pa || ts(b) - ts(a);
    });
}

/** Sem acento nem caixa. */
export function normalizarTexto(s: string): string {
  return s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

/** Todas as palavras da busca precisam aparecer em algum dos campos. */
export function casaBusca(campos: Array<string | null | undefined>, busca: string): boolean {
  const palavras = normalizarTexto(busca.trim()).split(/\s+/).filter(Boolean);
  if (palavras.length === 0) return true;
  const palheiro = normalizarTexto(campos.filter(Boolean).join(" "));
  return palavras.every((p) => palheiro.includes(p));
}

// ---------------------------------------------------------------------------

export interface Contagens {
  croquis: number;
  videos: number;
  audios?: number;
  image_analyses: number;
}

function plural(n: number, um: string, varios: string): string | null {
  return n > 0 ? `${n} ${n === 1 ? um : varios}` : null;
}

/** "1 croqui · 3 vídeos"; vazio quando não há nada. */
export function resumoConteudo(c: Contagens | null | undefined): string | null {
  if (!c) return null;
  const partes = [
    plural(c.croquis, "croqui", "croquis"),
    plural(c.videos, "vídeo", "vídeos"),
    plural(c.audios ?? 0, "áudio", "áudios"),
    plural(c.image_analyses, "imagem", "imagens"),
  ].filter(Boolean);
  return partes.length ? partes.join(" · ") : "vazio";
}

// ---------------------------------------------------------------------------

/** Quantos dias sem backup até virar "atrasado". */
export const BACKUP_ATRASADO_DIAS = 7;

export type EstadoBackup = "em-dia" | "atrasado" | "nunca";

export function estadoBackup(ultimoIso: string | null, agora: Date): EstadoBackup {
  if (!ultimoIso) return "nunca";
  const t = new Date(ultimoIso).getTime();
  if (Number.isNaN(t)) return "nunca";
  return agora.getTime() - t > BACKUP_ATRASADO_DIAS * 86_400_000 ? "atrasado" : "em-dia";
}

/** "hoje, 01:12", "ontem, 23:40" ou "09/11/2026, 18:05". */
export function rotuloBackup(ultimoIso: string, agora: Date): string {
  const d = new Date(ultimoIso);
  if (Number.isNaN(d.getTime())) return "—";
  const hora = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const dia0 = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const dif = Math.round((dia0(agora) - dia0(d)) / 86_400_000);
  if (dif === 0) return `hoje, ${hora}`;
  if (dif === 1) return `ontem, ${hora}`;
  return `${dmy(d)}, ${hora}`;
}
