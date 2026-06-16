/**
 * DossieModule — módulo "Dossiê" do caso, com DOIS modos.
 *
 *   ┌──────────────────────────────────────────────────────────────┐
 *   │ Dossiê — BO 42/2026 — Macapá        [Operacional][Integridade]│
 *   ├──────────────────────────────────────────────────────────────┤
 *   │ Resumo · Fotos · Checklist · …   (abas do modo ativo)        │
 *   ├──────────────────────────────────────────────────────────────┤
 *   │  <conteúdo do modo ativo>                                    │
 *   └──────────────────────────────────────────────────────────────┘
 *
 *   • Operacional  — o que o campo coletou (pacote .sicroapp): resumo,
 *     fotos, checklist, entidades, vestígios, medições, observações,
 *     timeline e a auditoria da importação.
 *   • Integridade  — camada de confiança/custódia: agrega TODA a evidência
 *     do workspace e verifica integridade em disco (hash, links, caminhos),
 *     com relatório de integridade.
 *
 * Antes eram dois itens separados na barra lateral ("Dossiê" e "Evidências").
 * Foram unificados aqui para reduzir a poluição da navegação e manter o fluxo
 * pericial coeso — sem perder nenhuma funcionalidade. O cabeçalho (título da
 * ocorrência + seletor de modo) vive aqui; cada modo é um painel que renderiza
 * só a sua faixa de abas + conteúdo.
 *
 * Deep-link: `/dossie?modo=integridade` abre direto no modo Integridade
 * (usado por atalhos como "Verificar integridade" na Home).
 */

import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { FolderOpen, ShieldCheck } from "lucide-react";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { useShortcuts } from "@core/useShortcuts";
import { CaseHeader } from "./CaseHeader";
import { OperacionalPanel } from "./OperacionalPanel";
import { IntegridadePanel } from "@modules/evidencias/IntegridadePanel";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import styles from "./DossieModule.module.css";

type Mode = "operacional" | "integridade";

export function DossieModule() {
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const [searchParams] = useSearchParams();
  const [mode, setMode] = useState<Mode>(
    searchParams.get("modo") === "integridade" ? "integridade" : "operacional",
  );

  // Atalhos: alternar entre as duas lentes (customizável em Configurações).
  useShortcuts({
    "dossie.lens.operacional": () => setMode("operacional"),
    "dossie.lens.provas": () => setMode("integridade"),
  });

  if (!workspacePath || !occurrence) {
    return (
      <NoOccurrenceState
        icon={<FolderOpen size={36} strokeWidth={1.5} />}
        moduleName="Dossiê"
      />
    );
  }

  return (
    <div className={styles.wrap}>
      <header className={styles.topBar}>
        <div className={styles.title}>
          <h1>Dossiê — {occurrenceHeader(occurrence)}</h1>
          <p className={styles.subtitle}>
            {mode === "operacional"
              ? "SICRO Operacional — a ponte de coleta do campo (pacote .sicroapp)"
              : "Central de Provas — todas as provas do caso, com acesso, metadados e cadeia de custódia"}
          </p>
        </div>
      </header>

      {/* Cabeçalho do caso: identificação SEMPRE editável (casos de expediente
          nascem aqui no Desktop, sem coleta de campo). Vale para as duas lentes. */}
      <CaseHeader occurrence={occurrence} />

      {/* Seletor de lente em destaque — a Central de Provas é tão importante
          quanto a coleta de campo, então as duas ganham o mesmo peso visual. */}
      <div className={styles.lensBand}>
        <div
          className={styles.lensToggle}
          role="tablist"
          aria-label="Lente do dossiê"
        >
          <button
            type="button"
            role="tab"
            aria-selected={mode === "operacional"}
            className={`${styles.lensBtn} ${
              mode === "operacional" ? styles.lensBtnActive : ""
            }`}
            onClick={() => setMode("operacional")}
          >
            <FolderOpen size={18} aria-hidden />
            <span className={styles.lensBtnText}>
              <strong>SICRO Operacional</strong>
              <small>Coleta do campo (mobile → desktop)</small>
            </span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={mode === "integridade"}
            className={`${styles.lensBtn} ${
              mode === "integridade" ? styles.lensBtnActive : ""
            }`}
            onClick={() => setMode("integridade")}
          >
            <ShieldCheck size={18} aria-hidden />
            <span className={styles.lensBtnText}>
              <strong>Central de Provas</strong>
              <small>Tudo produzido no SICRO + custódia</small>
            </span>
          </button>
        </div>
      </div>

      {mode === "operacional" ? (
        <OperacionalPanel workspacePath={workspacePath} />
      ) : (
        <IntegridadePanel workspacePath={workspacePath} />
      )}
    </div>
  );
}

function occurrenceHeader(
  o: NonNullable<ReturnType<typeof selectActiveOccurrence>>,
): string {
  const parts: string[] = [];
  if (o.numero_bo) parts.push(`BO ${o.numero_bo}`);
  if (o.tipo_pericia) parts.push(o.tipo_pericia);
  if (o.municipio) parts.push(o.municipio);
  if (parts.length === 0) return `Ocorrência ${o.id.slice(0, 8)}`;
  return parts.join(" — ");
}
