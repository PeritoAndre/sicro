/**
 * Atualização pelo próprio app. O `latest.json` sai na release do GitHub (release.yml);
 * o plugin confere a versão e a assinatura, baixa e reinstala. No Linux (.deb) e nas
 * compilações do fonte o plugin não instala: avisa e abre a página da versão.
 */

import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { open as abrirUrl } from "@tauri-apps/plugin-shell";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";
import { pushToast } from "@/components/toast/toastStore";

const RELEASES_API = "https://api.github.com/repos/PeritoAndre/sicro/releases/latest";
const RELEASES_PAGE = "https://github.com/PeritoAndre/sicro/releases/latest";

export type UpdaterStatus = "idle" | "checking" | "none" | "available" | "downloading" | "installed" | "error";

interface UpdaterState {
  status: UpdaterStatus;
  /** Versão instalada. */
  atual: string;
  /** Versão nova, se houver. */
  nova: string;
  notas: string;
  /** Baixado / total (0 a 1). */
  progresso: number;
  /** Instala sozinho (plugin) ou só abre a página de download. */
  instalavel: boolean;
  erro: string;
  /** Fechou o aviso: não insiste nesta sessão. */
  adiada: boolean;
  /** O diálogo está à vista (a versão nova segue conhecida mesmo com ele fechado). */
  aviso: boolean;
}

export const useUpdaterStore = create<UpdaterState>(() => ({
  status: "idle",
  atual: "",
  nova: "",
  notas: "",
  progresso: 0,
  instalavel: false,
  erro: "",
  adiada: false,
  aviso: false,
}));

let pendente: Update | null = null;
const set = useUpdaterStore.setState;

/** "5.4.0" > "5.3.2". */
export function versaoMaisNova(a: string, b: string): boolean {
  const pa = a.split(".").map((n) => parseInt(n, 10) || 0);
  const pb = b.split(".").map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = pa[i] ?? 0;
    const y = pb[i] ?? 0;
    if (x !== y) return x > y;
  }
  return false;
}

/** Sem o plugin (Linux .deb, fonte): pergunta à API do GitHub qual é a última release publicada. */
async function consultarGitHub(atual: string): Promise<{ versao: string; notas: string } | null> {
  const r = await fetch(RELEASES_API, { headers: { Accept: "application/vnd.github+json" } });
  if (!r.ok) throw new Error(`GitHub respondeu ${r.status}`);
  const j = (await r.json()) as { tag_name?: string; body?: string };
  const versao = (j.tag_name ?? "").replace(/^v/, "");
  if (!versao || !versaoMaisNova(versao, atual)) return null;
  return { versao, notas: j.body ?? "" };
}

/** `silencioso`: na abertura do app, só fala se houver versão nova. */
export async function verificarAtualizacao(silencioso: boolean): Promise<void> {
  const st = useUpdaterStore.getState();
  if (st.status === "checking" || st.status === "downloading") return;
  if (silencioso && st.adiada) return;
  const atual = st.atual || (await getVersion().catch(() => ""));
  set({ status: "checking", atual, erro: "" });
  try {
    let u: Update | null = null;
    let semPlugin = false;
    try {
      u = await check({ timeout: 15000 });
    } catch (e) {
      // Plataforma sem instalador assinado (linux .deb) ou build do fonte: cai na consulta simples.
      semPlugin = true;
      if (!/platform|target|unsupported|AppImage|not found/i.test(String(e))) throw e;
    }
    if (u) {
      pendente = u;
      set({ status: "available", nova: u.version, notas: u.body ?? "", instalavel: true, progresso: 0, aviso: true });
      return;
    }
    if (semPlugin) {
      const g = await consultarGitHub(atual);
      if (g) {
        set({ status: "available", nova: g.versao, notas: g.notas, instalavel: false, progresso: 0, aviso: true });
        return;
      }
    }
    set({ status: "none" });
    if (!silencioso) pushToast("success", `Você já está na versão mais recente (${atual}).`);
  } catch (e) {
    set({ status: "error", erro: String(e) });
    if (!silencioso) pushToast("error", "Não deu para verificar atualizações agora.", { title: "Atualização" });
  }
}

export async function instalarAtualizacao(): Promise<void> {
  const st = useUpdaterStore.getState();
  if (!st.instalavel || !pendente) {
    await abrirUrl(RELEASES_PAGE);
    return;
  }
  set({ status: "downloading", progresso: 0, aviso: true });
  let total = 0;
  let feito = 0;
  try {
    await pendente.downloadAndInstall((ev) => {
      if (ev.event === "Started") total = ev.data.contentLength ?? 0;
      else if (ev.event === "Progress") {
        feito += ev.data.chunkLength;
        if (total > 0) set({ progresso: Math.min(1, feito / total) });
      } else if (ev.event === "Finished") set({ progresso: 1 });
    });
    set({ status: "installed" });
    // No Windows o instalador já fecha e reabre o app; nos outros, reinicia aqui.
    await relaunch().catch(() => undefined);
  } catch (e) {
    set({ status: "error", erro: String(e) });
    pushToast("error", `A atualização falhou: ${String(e)}`, { title: "Atualização" });
  }
}

/** Fecha o diálogo; a versão nova continua anunciada na tela Início. */
export function adiarAtualizacao(): void {
  set({ aviso: false, adiada: true });
}

/** Abre o diálogo da versão nova sem baixar nada. */
export function mostrarNovidades(): void {
  if (useUpdaterStore.getState().status === "available") set({ aviso: true });
}
