/**
 * EntitiesTab — exibe veículos e vítimas/corpos em duas seções.
 * O backend grava em uma única tabela polimórfica `entities` com
 * `type` ∈ {vehicle, victim}. Detalhes específicos vivem em raw_json.
 */

import { useMemo } from "react";
import { Car, User } from "lucide-react";
import { commands } from "@core/commands";
import type { Entity } from "@domain/dossie";
import shared from "./shared.module.css";
import styles from "./EntitiesTab.module.css";
import { useDossieList } from "./useDossieList";

const VEHICLE_FIELDS: Array<[string, string]> = [
  ["identificador", "Identificador"],
  ["placa", "Placa"],
  ["tipo", "Tipo"],
  ["modelo", "Modelo"],
  ["cor", "Cor"],
  ["trafficDirection", "Sentido"],
  ["sentido_trafego", "Sentido"],
  ["finalPosition", "Posição final"],
  ["posicao_final", "Posição final"],
  ["ponto_impacto", "Ponto de impacto"],
  ["danos", "Danos"],
  ["motorista", "Motorista"],
  ["proprietario", "Proprietário"],
  ["observacao", "Observação"],
];

const VICTIM_FIELDS: Array<[string, string]> = [
  ["identificador", "Identificador"],
  ["nome", "Nome"],
  ["condicao", "Condição"],
  ["tipo", "Tipo"],
  ["status_remocao", "Status de remoção"],
  ["removalStatus", "Status de remoção"],
  ["resgatado_por", "Removido por"],
  ["rescuedBy", "Removido por"],
  ["destino", "Destino"],
  ["destination", "Destino"],
  ["posicao_corpo", "Posição do corpo"],
  ["bodyPosition", "Posição do corpo"],
  ["equipamentos_protecao", "Equipamentos de proteção"],
  ["protectiveEquipment", "Equipamentos de proteção"],
  ["observacao", "Observação"],
];

export function EntitiesTab({ workspacePath }: { workspacePath: string }) {
  const { items, loading, error } = useDossieList(workspacePath, commands.listDossieEntities);

  const vehicles = useMemo(() => items.filter((e) => e.type === "vehicle"), [items]);
  const victims = useMemo(() => items.filter((e) => e.type === "victim"), [items]);

  if (loading && items.length === 0) return <p className={shared.dim}>Carregando entidades…</p>;
  if (error) return <p className={shared.error}>{error}</p>;
  if (vehicles.length === 0 && victims.length === 0) {
    return (
      <div className={shared.empty}>
        <User size={28} aria-hidden />
        <span>O pacote não trouxe veículos nem vítimas.</span>
      </div>
    );
  }

  return (
    <div className={shared.tab}>
      <Section
        title={`Veículos (${vehicles.length})`}
        icon={<Car size={14} />}
        entities={vehicles}
        fields={VEHICLE_FIELDS}
        emptyText="Sem veículos importados."
      />
      <Section
        title={`Vítimas / corpos (${victims.length})`}
        icon={<User size={14} />}
        entities={victims}
        fields={VICTIM_FIELDS}
        emptyText="Sem vítimas/corpos importados."
      />
    </div>
  );
}

function Section({
  title,
  icon,
  entities,
  fields,
  emptyText,
}: {
  title: string;
  icon: React.ReactNode;
  entities: Entity[];
  fields: Array<[string, string]>;
  emptyText: string;
}) {
  return (
    <section className={shared.card}>
      <header className={shared.cardHeader}>
        <h2 className={shared.cardTitle}>
          <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
            {icon} {title}
          </span>
        </h2>
      </header>
      {entities.length === 0 ? (
        <p className={shared.dim} style={{ fontSize: "var(--text-sm)" }}>
          {emptyText}
        </p>
      ) : (
        <div className={styles.list}>
          {entities.map((e) => (
            <EntityCard key={e.id} entity={e} fields={fields} />
          ))}
        </div>
      )}
    </section>
  );
}

function EntityCard({ entity, fields }: { entity: Entity; fields: Array<[string, string]> }) {
  const raw = useMemo<Record<string, unknown>>(() => {
    try {
      return JSON.parse(entity.raw_json);
    } catch {
      return {};
    }
  }, [entity.raw_json]);
  const photoIds = useMemo<string[]>(() => {
    try {
      return JSON.parse(entity.photo_ids_json);
    } catch {
      return [];
    }
  }, [entity.photo_ids_json]);

  const rows: Array<[string, string]> = [];
  const seenLabels = new Set<string>();
  for (const [key, label] of fields) {
    if (seenLabels.has(label)) continue;
    const v = raw[key];
    if (v === null || v === undefined) continue;
    const s = typeof v === "string" ? v.trim() : String(v);
    if (!s) continue;
    rows.push([label, s]);
    seenLabels.add(label);
  }

  return (
    <article className={styles.card}>
      <header className={styles.entityHeader}>
        <strong>{entity.identifier ?? entity.original_id ?? entity.id.slice(0, 8)}</strong>
        {entity.label && entity.label !== entity.identifier && (
          <span className={shared.dim} style={{ fontSize: "var(--text-xs)" }}>
            {entity.label}
          </span>
        )}
      </header>
      {rows.length === 0 ? (
        <p className={shared.dim} style={{ fontSize: "var(--text-xs)" }}>
          Sem campos estruturados disponíveis. Raw JSON preservado.
        </p>
      ) : (
        <dl className={shared.metaGrid}>
          {rows.map(([label, value]) => (
            <Row key={label} label={label} value={value} />
          ))}
        </dl>
      )}
      {photoIds.length > 0 && (
        <div className={styles.photoHints}>
          <span className={shared.dim} style={{ fontSize: "var(--text-xs)" }}>
            Fotos vinculadas:
          </span>
          {photoIds.map((id) => (
            <code key={id} className={shared.mono}>
              {id}
            </code>
          ))}
        </div>
      )}
    </article>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </>
  );
}
