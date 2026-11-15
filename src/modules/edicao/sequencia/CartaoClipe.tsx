/**
 * Cartão de um item da trilha principal (§8.2, §8.6): número, tipo, nome, duração, selos, problema da validação
 * numa linha curta e os itens presos a ele (`Presos`, que lê a store pelo `id`). Item de versão futura ou inválido:
 * cartão apagado com o `tipo`, sem editor. Estado só por ícone; os rótulos são os do §8.6.
 */

import type { PointerEvent as PE } from "react";
import { CircleAlert, Copy, Eye, GripVertical, Trash2 } from "lucide-react";
import type { OrigemDoClipe } from "../store/origemStore";
import type { Id, Problema, Seg } from "@domain/edicao";
import { textoDuracao, type RotuloTipo, type Selo } from "./contas";
import { Presos, PRESOS_PRONTOS } from "./Presos";
import styles from "./Sequencia.module.css";

// CONTRATO: saem `presos`, `onAdicionar`, `onPreso` (os presos são do `Presos`); entra `origem?` (contratos-fase1 F8).
export interface CartaoProps {
  /** Posição na trilha (0…). */
  indice: number;
  /** Id do clipe; ausente no item sem id. */
  id?: Id;
  /** null = item desconhecido ou inválido; o texto é o `tipo` cru. */
  tipo: RotuloTipo | null;
  tipoCru: string;
  nome: string;
  duracao: Seg | null;
  selos: readonly Selo[];
  problemas: readonly Problema[];
  selecionado: boolean;
  desligado: boolean;
  arrastando: boolean;
  soltar: "antes" | "depois" | null;
  soLeitura: boolean;
  /** Intermediário de origem do clipe (§7.5): em preparo mostra o progresso; falhou, o ícone. */
  origem?: OrigemDoClipe;
  onSelecionar(): void;
  onVer(): void;
  onDuplicar(): void;
  onRemover(): void;
  onAlca(e: PE<HTMLElement>): void;
}

export function CartaoClipe(p: CartaoProps) {
  const conhecido = p.tipo !== null;
  // erro primeiro; senão o primeiro aviso
  const problema = p.problemas.find((x) => x.nivel === "erro") ?? p.problemas[0] ?? null;
  const classes = [
    styles.cartao,
    p.selecionado ? styles.selecionado : "",
    !conhecido || p.desligado ? styles.apagado : "",
    problema?.nivel === "erro" ? styles.ruim : "",
    p.arrastando ? styles.arrastando : "",
    p.soltar === "antes" ? styles.soltarAntes : p.soltar === "depois" ? styles.soltarDepois : "",
  ]
    .filter(Boolean)
    .join(" ");
  const parar = (e: { stopPropagation(): void }) => e.stopPropagation();
  return (
    <li className={classes} onClick={conhecido ? p.onSelecionar : undefined} data-cartao={p.indice} aria-selected={p.selecionado}>
      <div className={styles.topo}>
        {!p.soLeitura && (
          <span className={styles.alca} onPointerDown={p.onAlca} onClick={parar} aria-hidden>
            <GripVertical size={14} />
          </span>
        )}
        <span className={styles.num}>{p.indice + 1}</span>
        <strong className={styles.tipo}>{p.tipo ?? p.tipoCru}</strong>
        {p.nome && <span className={styles.nome}>{p.nome}</span>}
        {p.duracao !== null && <span className={styles.dur}>{textoDuracao(p.duracao)}</span>}
        {p.origem?.tipo === "falhou" && (
          <span className={styles.origemFalhou} title={p.origem.erro.frase || p.origem.erro.codigo || undefined} data-origem="falhou">
            <CircleAlert size={12} aria-hidden />
          </span>
        )}
      </div>
      {p.origem?.tipo === "fazendo" && (
        <span className={styles.origemTrilho} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(p.origem.feito * 100)} data-origem="fazendo">
          <span className={styles.origemBarra} style={{ width: `${Math.round(Math.min(Math.max(p.origem.feito, 0), 1) * 100)}%` }} />
        </span>
      )}
      {(p.selos.length > 0 || conhecido || !p.soLeitura) && (
        <div className={styles.linha2}>
          <span className={styles.selos}>
            {p.selos.map((s) => {
              const texto = typeof s === "string" ? s : s.velocidade;
              const ruim = s === "Vídeo ausente" || s === "Danificado";
              return (
                <span key={texto} className={`${styles.selo} ${ruim ? styles.seloRuim : ""}`}>
                  {texto}
                </span>
              );
            })}
          </span>
          <span className={styles.acoes} onClick={parar}>
            {conhecido && (
              <button type="button" title="Ver" aria-label="Ver" onClick={p.onVer}>
                <Eye size={13} />
              </button>
            )}
            {conhecido && !p.soLeitura && (
              <button type="button" title="Duplicar" aria-label="Duplicar" onClick={p.onDuplicar}>
                <Copy size={13} />
              </button>
            )}
            {!p.soLeitura && (
              <button type="button" title="Remover" aria-label="Remover" onClick={p.onRemover}>
                <Trash2 size={13} />
              </button>
            )}
          </span>
        </div>
      )}
      {problema && (
        <p className={problema.nivel === "erro" ? styles.erroLinha : styles.avisoLinha} data-problema={problema.codigo}>
          {problema.mensagem}
        </p>
      )}
      {conhecido && PRESOS_PRONTOS && p.id && <Presos clipe={p.id} />}
    </li>
  );
}
