/**
 * LaudoBridgeView — a "ponte" entre o laudo `.docx` (aberto no Word) e os
 * dados/artefatos da ocorrência (SICRO 3.0).
 *
 * O laudo deixou de ter editor in-app: o perito escreve no Word/LibreOffice.
 * Esta tela é o COMPANHEIRO da redação — três áreas:
 *
 *   - Barra superior: título do laudo + "Abrir no Word" + voltar pra lista.
 *   - Consulta: campos-chave da ocorrência (resolvidos pelo catálogo de campos),
 *     agrupados e somente-leitura, pro perito consultar enquanto escreve.
 *   - Produção: artefatos de imagem da ocorrência (fotos, exports de croqui,
 *     frames, imagens de análise) com botão "Copiar pro laudo" — copia a imagem
 *     pra área de transferência pro perito colar (Ctrl+V) no `.docx`.
 *
 * MVP: a Produção lista os itens do registro central de evidências
 * (`verify_workspace_integrity`) e filtra os que têm imagem; a mecânica de
 * cópia (`copyArtifact` → `copy_image_to_clipboard`) está ligada de ponta a
 * ponta. Refinos (busca, agrupamento por módulo, ordenação) ficam pra Fase B.
 */

import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Clipboard, FileText, ImageIcon, RefreshCw } from "lucide-react";
import { Button } from "@components/Button/Button";
import { toSicroError } from "@core/errors";
import { commands } from "@core/commands";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { useLaudoStore } from "../store/laudoStore";
import {
  FIELD_GROUPS,
  LAUDO_FIELDS,
  groupLabel,
  resolveAllFields,
  type LaudoFieldGroup,
} from "../document-engine";
import { assetUrl } from "../../evidencias/shared";
import type { EvidenceRegistryItem } from "@domain/evidence_registry";
import type { Laudo } from "@domain/laudo";
import type { Occurrence } from "@domain/occurrence";
import styles from "./LaudoBridgeView.module.css";

interface LaudoBridgeViewProps {
  workspacePath: string;
  laudo: Laudo;
  onBack: () => void;
}

/** Tipos de evidência que costumam ser imagens "coláveis" no laudo. */
const IMAGE_KINDS = new Set([
  "photo",
  "croqui_export",
  "video_frame",
  "storyboard_frame",
  "image_analysis",
  "image_export",
]);

/** True quando o item é uma imagem que dá pra copiar pro Word. */
function isCopyableImage(item: EvidenceRegistryItem): boolean {
  if (!item.relative_path) return false;
  if (item.mime_type?.startsWith("image/")) return true;
  if (/\.(png|jpe?g|webp|bmp|gif)$/i.test(item.relative_path)) return true;
  return IMAGE_KINDS.has(item.kind);
}

export function LaudoBridgeView({
  workspacePath,
  laudo,
  onBack,
}: LaudoBridgeViewProps) {
  const occurrence = useWorkspaceStore((s) => s.activeOccurrence);
  const openExternal = useLaudoStore((s) => s.openExternal);
  const copyArtifact = useLaudoStore((s) => s.copyArtifact);

  const [items, setItems] = useState<EvidenceRegistryItem[]>([]);
  const [loadingArtifacts, setLoadingArtifacts] = useState(false);
  const [artifactError, setArtifactError] = useState<string | null>(null);
  const [copyingPath, setCopyingPath] = useState<string | null>(null);

  const loadArtifacts = useMemo(
    () => async () => {
      setLoadingArtifacts(true);
      setArtifactError(null);
      try {
        // Registro central de evidências: já consolida fotos/croquis/frames/
        // imagens de análise da ocorrência com `relative_path` + `mime_type`.
        const report = await commands.verifyWorkspaceIntegrity(workspacePath);
        setItems(report.items.filter(isCopyableImage));
      } catch (err) {
        setArtifactError(toSicroError(err).message);
      } finally {
        setLoadingArtifacts(false);
      }
    },
    [workspacePath],
  );

  useEffect(() => {
    void loadArtifacts();
  }, [loadArtifacts]);

  const consulta = useMemo(
    () => buildConsultaGroups(occurrence),
    [occurrence],
  );

  const handleCopy = async (item: EvidenceRegistryItem) => {
    if (!item.relative_path) return;
    setCopyingPath(item.relative_path);
    try {
      await copyArtifact(item.relative_path);
    } finally {
      setCopyingPath(null);
    }
  };

  return (
    <div className={styles.wrap}>
      <header className={styles.topbar}>
        <Button variant="ghost" leftIcon={<ArrowLeft size={16} />} onClick={onBack}>
          Laudos
        </Button>
        <div className={styles.titleBlock}>
          <FileText size={18} />
          <h1 className={styles.title} title={laudo.title}>
            {laudo.title}
          </h1>
        </div>
        <Button
          variant="primary"
          leftIcon={<FileText size={16} />}
          onClick={() => void openExternal(laudo.id)}
        >
          Abrir no Word
        </Button>
      </header>

      <p className={styles.hint}>
        O laudo é um documento <code>.docx</code> editado no Word ou LibreOffice.
        Use o painel "Consulta" para conferir os dados do caso e o painel
        "Produção" para enviar imagens ao documento.
      </p>

      <div className={styles.panels}>
        {/* ---- Consulta: dados da ocorrência (somente-leitura) ---- */}
        <section className={styles.panel}>
          <h2 className={styles.panelTitle}>Consulta</h2>
          {consulta.length === 0 ? (
            <p className={styles.empty}>
              Sem dados da ocorrência para exibir.
            </p>
          ) : (
            consulta.map((group) => (
              <div key={group.group} className={styles.group}>
                <h3 className={styles.groupTitle}>{groupLabel(group.group)}</h3>
                <dl className={styles.fieldList}>
                  {group.fields.map((f) => (
                    <div key={f.key} className={styles.fieldRow}>
                      <dt className={styles.fieldLabel}>{f.label}</dt>
                      <dd className={styles.fieldValue}>{f.value}</dd>
                    </div>
                  ))}
                </dl>
              </div>
            ))
          )}
        </section>

        {/* ---- Produção: artefatos de imagem coláveis ---- */}
        <section className={styles.panel}>
          <div className={styles.panelHead}>
            <h2 className={styles.panelTitle}>Produção</h2>
            <Button
              variant="ghost"
              size="sm"
              leftIcon={<RefreshCw size={14} />}
              onClick={() => void loadArtifacts()}
              disabled={loadingArtifacts}
            >
              {loadingArtifacts ? "Atualizando…" : "Atualizar"}
            </Button>
          </div>

          {artifactError && <p className={styles.error}>{artifactError}</p>}

          {!artifactError && items.length === 0 && !loadingArtifacts && (
            <p className={styles.empty}>
              Nenhuma imagem disponível na ocorrência (fotos, croqui exportado,
              frames ou imagens de análise).
            </p>
          )}

          <ul className={styles.artifactList}>
            {items.map((item) => {
              const thumb = item.relative_path
                ? assetUrl(workspacePath, item.relative_path)
                : null;
              return (
                <li key={item.id} className={styles.artifactRow}>
                  <div className={styles.thumb}>
                    {thumb ? (
                      <img src={thumb} alt="" loading="lazy" />
                    ) : (
                      <ImageIcon size={20} />
                    )}
                  </div>
                  <div className={styles.artifactMeta}>
                    <span className={styles.artifactName}>
                      {item.title ?? item.relative_path}
                    </span>
                    <code className={styles.artifactPath}>
                      {item.relative_path}
                    </code>
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    leftIcon={<Clipboard size={14} />}
                    onClick={() => void handleCopy(item)}
                    disabled={copyingPath === item.relative_path}
                  >
                    {copyingPath === item.relative_path
                      ? "Copiando…"
                      : "Copiar pro laudo"}
                  </Button>
                </li>
              );
            })}
          </ul>
        </section>
      </div>
    </div>
  );
}

interface ConsultaField {
  key: string;
  label: string;
  value: string;
}

interface ConsultaGroup {
  group: LaudoFieldGroup;
  fields: ConsultaField[];
}

/**
 * Resolve os campos do catálogo a partir da ocorrência ativa e agrupa pela
 * categoria do campo, descartando os vazios e o grupo "sistema" (data atual /
 * contadores de página — irrelevantes pra consulta do caso).
 */
function buildConsultaGroups(
  occurrence: Occurrence | null,
): ConsultaGroup[] {
  const resolved = resolveAllFields(
    {
      metadata: {},
      occurrence: (occurrence ?? null) as Record<string, unknown> | null,
    },
    LAUDO_FIELDS,
  );

  const groups: ConsultaGroup[] = [];
  for (const group of FIELD_GROUPS) {
    if (group === "sistema") continue;
    const fields: ConsultaField[] = [];
    for (const def of LAUDO_FIELDS) {
      if (def.group !== group) continue;
      const value = (resolved.get(def.key) ?? "").trim();
      if (!value) continue;
      fields.push({ key: def.key, label: def.label, value });
    }
    if (fields.length > 0) groups.push({ group, fields });
  }
  return groups;
}
