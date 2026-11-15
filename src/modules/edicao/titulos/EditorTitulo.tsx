/**
 * Prévia do título no Inspetor (P1.7, §5.5 e D8): o título no canvas da SAÍDA, com os mesmos sprites que o render
 * usa (o pedido de cada bloco é o de `camada::titulo::pedido`, conferido pela fixture: mesmo arquivo no cache, mesmo
 * SHA-256), a área segura tracejada e, quando um bloco sai dela, o bloco contornado e um ícone. Sem texto na tela.
 */

import { useEffect, useState } from "react";
import { TriangleAlert } from "lucide-react";
import type { ClipeTitulo, PedidoSprite, SobTitulo, SpriteTexto, Titulo } from "@domain/edicao";
import { useEdicaoStore } from "../store/edicaoStore";
import { spriteDe, urlDoCache, useRecursos } from "../palco/recursos";
import { AREA_SEGURA, blocosNoCanvas, naAreaSegura, type BlocoNoCanvas } from "../modelo/titulos";
import styles from "./EditorTitulo.module.css";

export const EDITOR_TITULO_PRONTO = true;

const pct = (v: number) => `${v * 100}%`;

/** Blocos do título postos no canvas com os sprites que já chegaram (pedido ao backend na primeira vez). */
export function blocosDaPrevia(t: Titulo, canvas: { w: number; h: number }, sprite: (p: PedidoSprite) => SpriteTexto | null): BlocoNoCanvas[] {
  try {
    return blocosNoCanvas(t, { canvas, sprite, midia: () => null });
  } catch {
    // lugar inválido: a validação acusa no campo; a prévia fica sem os blocos
    return [];
  }
}

/** Sprite do pedido, se já chegou; o primeiro pedido vai ao backend. Falha ou resposta ruim: sem sprite. */
function spriteDoCaso(ws: string | null, p: PedidoSprite): SpriteTexto | null {
  if (!ws) return null;
  try {
    const s = spriteDe(ws, p);
    return s && typeof s.relativo === "string" && s.w > 0 && s.h > 0 ? s : null;
  } catch {
    return null;
  }
}

export function EditorTitulo({ clipe }: { clipe: ClipeTitulo | SobTitulo }) {
  const ws = useEdicaoStore((s) => s.ws);
  const raiz = useEdicaoStore((s) => s.raizCache);
  const saida = useEdicaoStore((s) => s.doc?.saida);
  // sprite que chega repinta
  const versao = useRecursos((s) => s.versao);
  const [blocos, setBlocos] = useState<BlocoNoCanvas[]>([]);
  const t = clipe.titulo;
  const [w, h] = [saida?.largura ?? 0, saida?.altura ?? 0];
  const valido = w > 0 && h > 0;
  // pedido fora do render: o primeiro pedido de um caso troca o estado dos recursos
  useEffect(() => {
    setBlocos(valido ? blocosDaPrevia(t, { w, h }, (p) => spriteDoCaso(ws, p)) : []);
  }, [t, w, h, valido, ws, versao]);
  if (!valido) return null;
  const fora = new Set(blocos.filter((b) => !naAreaSegura(b.caixa)).map((b) => b.id));
  const url = (src: string) => {
    if (!raiz) return null;
    try {
      return urlDoCache(raiz, src);
    } catch {
      return null;
    }
  };
  const f = t.fundo;
  return (
    <div
      className={`${styles.previa} ${f.tipo === "cor" ? "" : styles.xadrez}`}
      style={{ aspectRatio: `${w} / ${h}`, ...(f.tipo === "cor" ? { background: f.cor } : {}) }}
      data-previa-titulo=""
      data-canvas={`${w}x${h}`}
    >
      {blocos.map((b) => {
        const u = url(b.src);
        return (
          u && (
            <img
              key={b.id}
              className={`${styles.bloco} ${fora.has(b.id) ? styles.fora : ""}`}
              src={u}
              alt=""
              draggable={false}
              data-bloco={b.id}
              data-sprite={b.src}
              data-fora={fora.has(b.id) ? "" : undefined}
              style={{ left: pct(b.caixa.x), top: pct(b.caixa.y), width: pct(b.caixa.w), height: pct(b.caixa.h) }}
            />
          )
        );
      })}
      <div className={styles.area} style={{ inset: pct(AREA_SEGURA) }} data-area-segura="" aria-hidden />
      {fora.size > 0 && (
        <span className={styles.estado} data-estado="fora" aria-hidden>
          <TriangleAlert size={14} />
        </span>
      )}
    </div>
  );
}
