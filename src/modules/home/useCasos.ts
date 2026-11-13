/** Hooks de dados da tela Início: índice de casos, contagens por caso e abrir caso. */

import { useCallback, useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { pushToast } from "@/components/toast/toastStore";
import { useNavGuard } from "@/app/navGuard";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { WorkspaceCounters } from "@domain/alpha";
import type { CaseIndexEntry } from "@domain/case_index";
import type { RecentOccurrence } from "@domain/occurrence";
import { tituloDoCaso } from "./casosHome";
import { lastWorkModuleOf } from "@modules/midia/midiaNav";
import { selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";

/** Índice global por caminho do caso (best-effort: sem ele só faltam número e município). */
export function useIndiceCasos(): Map<string, CaseIndexEntry> {
  const versao = useWorkspaceStore((s) => s.caseIndexVersion);
  const [mapa, setMapa] = useState<Map<string, CaseIndexEntry>>(new Map());
  useEffect(() => {
    let cancelado = false;
    commands
      .getCaseIndex()
      .then((l) => {
        if (!cancelado) setMapa(new Map(l.map((e) => [e.workspace_path, e])));
      })
      .catch(() => {});
    return () => {
      cancelado = true;
    };
  }, [versao]);
  return mapa;
}

const cache = new Map<string, WorkspaceCounters>();
let cacheVersao = -1;

/** Contagens reais (com áudios) dos casos à vista, buscadas uma a uma e guardadas. */
export function useContagens(paths: string[]): Map<string, WorkspaceCounters> {
  const versao = useWorkspaceStore((s) => s.caseIndexVersion);
  const [, tick] = useState(0);
  const chave = paths.join("\n");
  useEffect(() => {
    if (cacheVersao !== versao) {
      cache.clear();
      cacheVersao = versao;
    }
    let cancelado = false;
    void (async () => {
      for (const p of chave.split("\n").filter(Boolean)) {
        if (cancelado) return;
        if (cache.has(p)) continue;
        try {
          cache.set(p, await commands.getOccurrenceCounts(p));
          if (!cancelado) tick((n) => n + 1);
        } catch {
          /* caso movido ou excluído */
        }
      }
    })();
    return () => {
      cancelado = true;
    };
  }, [chave, versao]);
  return cache;
}

/** Troca na hora: o caso atual é fechado sem perguntar e o escolhido abre no módulo em que parou. */
export function useAbrirCaso(): (path: string) => Promise<boolean> {
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const openOccurrence = useWorkspaceStore((s) => s.openOccurrence);
  const ativo = useWorkspaceStore(selectActiveWorkspacePath);

  return useCallback(
    (path: string) =>
      new Promise<boolean>((resolve) => {
        const executar = async () => {
          try {
            const loaded = await openOccurrence(path);
            const destino = lastWorkModuleOf(loaded.occurrence.id) ?? "/";
            if (destino !== pathname) navigate(destino);
            resolve(true);
          } catch (e) {
            pushToast("error", toSicroError(e).message);
            resolve(false);
          }
        };
        const guard = useNavGuard.getState();
        if (!guard.guard || path === ativo) void executar();
        else {
          void guard.attemptNavigation(() => void executar());
          // Se o guard recusar, nada abre; a tela de origem segue como estava.
          resolve(true);
        }
      }),
    [navigate, openOccurrence, pathname, ativo],
  );
}

/** Título de um recente: usa o índice (nome, número do BO) quando existir, senão o rótulo salvo. */
export function tituloRecente(r: RecentOccurrence, indice: Map<string, CaseIndexEntry>): string {
  const e = indice.get(r.workspace_path);
  if (!e || !(e.titulo?.trim() || e.numero_bo?.trim())) return r.occurrence_label;
  return tituloDoCaso({ titulo: e.titulo, numero_bo: e.numero_bo, created_at: e.created_at });
}
