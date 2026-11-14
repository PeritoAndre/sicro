/**
 * Cartão de um item da trilha principal (§8.2, §8.6): número, tipo, nome, duração, selos, problema da validação
 * numa linha curta e os itens presos a ele. Item de versão futura ou inválido: cartão apagado com o `tipo`, sem
 * editor. Estado só por ícone; os rótulos são os do §8.6.
 */

import { useEffect, useRef, useState, type PointerEvent as PE } from "react";
import { Copy, Eye, GripVertical, Image as IconeImagem, Mic, Music, Plus, Trash2, Type, Video, Volume2 } from "lucide-react";
import type { Id, Problema, Seg } from "@domain/edicao";
import { textoDuracao, type Preso, type RotuloTipo, type Selo } from "./contas";
import { useMenuAberto } from "../menus";
import { Presos, PRESOS_PRONTOS } from "./Presos";
import styles from "./Sequencia.module.css";

export type TipoPreso = "Título" | "Terço inferior" | "Imagem" | "Narração";
export const TIPOS_PRESO: readonly TipoPreso[] = ["Título", "Terço inferior", "Imagem", "Narração"];

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
  presos: readonly Preso[];
  selecionado: boolean;
  desligado: boolean;
  arrastando: boolean;
  soltar: "antes" | "depois" | null;
  soLeitura: boolean;
  onSelecionar(): void;
  onVer(): void;
  onDuplicar(): void;
  onRemover(): void;
  onAlca(e: PE<HTMLElement>): void;
  onAdicionar(t: TipoPreso): void;
  onPreso(p: Preso): void;
}

const ICONE_PRESO = { titulo: Type, imagem: IconeImagem, video: Video, narracao: Mic, musica: Music, efeitos: Volume2 } as const;
/** Nome do item preso sem rótulo próprio (só o ícone aparece): o do tipo. */
const NOME_PRESO = { titulo: "Título", imagem: "Imagem", video: "Vídeo", narracao: "Narração", musica: "Música", efeitos: "Efeitos" } as const;

export function CartaoClipe(p: CartaoProps) {
  const [menu, setMenu] = useState(false);
  const menuRef = useRef<HTMLSpanElement | null>(null);
  useMenuAberto(menu);
  // menu aberto: clique fora ou Esc fecha
  useEffect(() => {
    if (!menu) return;
    const fora = (e: Event) => {
      if (!menuRef.current?.contains(e.target as Node)) setMenu(false);
    };
    const tecla = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenu(false);
    };
    window.addEventListener("pointerdown", fora, true);
    window.addEventListener("keydown", tecla);
    return () => {
      window.removeEventListener("pointerdown", fora, true);
      window.removeEventListener("keydown", tecla);
    };
  }, [menu]);
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
      </div>
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
      {conhecido && !PRESOS_PRONTOS && (p.presos.length > 0 || p.selecionado) && (
        <div className={styles.presos} onClick={parar}>
          {p.presos.map((x) => {
            const I = ICONE_PRESO[x.tipo];
            return (
              <button type="button" key={x.id} className={styles.preso} onClick={() => p.onPreso(x)} title={x.rotulo || NOME_PRESO[x.tipo]} aria-label={x.rotulo || NOME_PRESO[x.tipo]}>
                <I size={12} aria-hidden />
                {x.rotulo && <span>{x.rotulo}</span>}
              </button>
            );
          })}
          {!p.soLeitura && (
            <span className={styles.adicionar} ref={menuRef}>
              <button type="button" className={styles.mini} onClick={() => setMenu((m) => !m)} aria-expanded={menu}>
                <Plus size={12} aria-hidden />
                Adicionar
              </button>
              {menu && (
                <span className={styles.menu} role="menu">
                  {TIPOS_PRESO.map((t) => (
                    <button
                      type="button"
                      role="menuitem"
                      key={t}
                      onClick={() => {
                        setMenu(false);
                        p.onAdicionar(t);
                      }}
                    >
                      {t}
                    </button>
                  ))}
                </span>
              )}
            </span>
          )}
        </div>
      )}
    </li>
  );
}
