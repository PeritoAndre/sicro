/**
 * Palco durante a corrida (§6.3, §8.2): mostra a miniatura do evento com a caixa e o estado (ícone de incerto ou
 * fora de cena), sem buscar no `<video>`, o tempo do quadro e "Parar". Com a ferramenta de marcar um rastreio
 * ligada, "Seguir para trás" e "Seguir" no próprio palco: a âncora marcada vira o pedido da corrida.
 * Dono: P2.4. Montada no `Palco` (com a bandeira `RASTREIO_PRONTO`).
 */

import { ChevronsLeft, ChevronsRight, Loader2, Square } from "lucide-react";
import type { CSSProperties } from "react";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { useEdicaoStore } from "../store/edicaoStore";
import { acharMidia, acharRastreio } from "../modelo/projeto";
import { recursoDe, useRecursos } from "../palco/recursos";
import { useInspetorUi } from "../inspetor/ui";
import { useOrigemStore } from "../store/origemStore";
import { alvoDoEfeito, pedidoDe, RASTREIO_PRONTO, seguirEfeito, seguirParado, useRastreioStore } from "../store/rastreioStore";
import { IconeEstado } from "./PainelRastreio";

export interface FerramentaSeguirProps {
  /** Parte visível do quadro no palco (px da caixa do palco; com zoom, só o que cabe na caixa). */
  area: { x: number; y: number; w: number; h: number };
}

const BARRA: CSSProperties = {
  position: "absolute",
  top: 8,
  left: "50%",
  transform: "translateX(-50%)",
  display: "flex",
  alignItems: "center",
  gap: 6,
  padding: "3px 6px",
  borderRadius: "var(--radius-sm, 4px)",
  background: "rgb(0 0 0 / 60%)",
  color: "#ddd",
  fontSize: "var(--text-xs, 11px)",
  whiteSpace: "nowrap",
  zIndex: 5,
};
const BOTAO: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 4,
  padding: "2px 7px",
  border: "1px solid rgb(255 255 255 / 25%)",
  borderRadius: "var(--radius-sm, 4px)",
  background: "rgb(0 0 0 / 35%)",
  color: "#eee",
  font: "inherit",
  cursor: "pointer",
};

/** Retângulo da imagem de proporção `p` contida em `w`×`h`. */
function contida(w: number, h: number, p: number) {
  const iw = Math.min(w, h * p);
  const ih = iw / p;
  return { x: (w - iw) / 2, y: (h - ih) / 2, w: iw, h: ih };
}

export function FerramentaSeguir({ area }: FerramentaSeguirProps) {
  const corrida = useRastreioStore((s) => s.corrida);
  const paradas = useRastreioStore((s) => s.paradas);
  const ferramenta = useInspetorUi((s) => s.ferramenta);
  const doc = useEdicaoStore((s) => s.doc);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  const cursor = useEdicaoStore((s) => s.cursor);
  // a tabela da mídia chega depois (tempo do quadro, de onde seguir)
  useRecursos((s) => s.versao);
  // intermediário de origem em preparo desliga "Seguir"
  useOrigemStore((s) => s.porClipe);
  if (!RASTREIO_PRONTO || !doc || area.w <= 0 || area.h <= 0) return null;

  if (corrida) {
    const r = acharRastreio(doc, corrida.rastreio);
    const m = r?.midia ? acharMidia(doc, r.midia) : null;
    const tab = recursoDe(m?.sha256).tabela;
    const u = corrida.ultimo;
    const img = contida(area.w, area.h, m?.largura && m.altura ? m.largura / m.altura : 16 / 9);
    return (
      <div style={{ position: "absolute", left: area.x, top: area.y, width: area.w, height: area.h, background: "#000", zIndex: 4 }} data-seguindo={corrida.rastreio}>
        {u?.miniatura && (
          <img src={u.miniatura} alt="" draggable={false} data-miniatura="" style={{ position: "absolute", left: img.x, top: img.y, width: img.w, height: img.h, objectFit: "fill" }} />
        )}
        {u?.caixa && (
          <div
            data-caixa-seguida=""
            style={{
              position: "absolute",
              left: img.x + u.caixa.x * img.w,
              top: img.y + u.caixa.y * img.h,
              width: u.caixa.w * img.w,
              height: u.caixa.h * img.h,
              border: `2px solid ${r?.cor ?? "#ffcc00"}`,
              boxSizing: "border-box",
              pointerEvents: "none",
            }}
          />
        )}
        <div style={BARRA}>
          {u ? <IconeEstado estado={u.estado} tamanho={14} /> : <Loader2 size={14} aria-hidden />}
          {u && tab && <span data-dado="">{formatarTempo((u.pts * tab.tb.num) / tab.tb.den)}</span>}
          <button type="button" style={BOTAO} onClick={() => void useRastreioStore.getState().parar()} data-acao="parar-rastreio">
            <Square size={12} aria-hidden /> Parar
          </button>
        </div>
      </div>
    );
  }

  // marcando um rastreio: seguir dali mesmo
  if (!ferramenta?.rastreio || !ferramenta.efeito || soLeitura) return null;
  const clipe = cursor.modo === "clipe" ? cursor.clipe : null;
  const x = alvoDoEfeito(doc, ferramenta.efeito, clipe);
  if (!x || x.rastreio.id !== ferramenta.rastreio) return null;
  const parado = seguirParado(doc, x.efeito.id);
  const tras = !parado && pedidoDe(x, "tras", "seguir", paradas);
  const frente = !parado && pedidoDe(x, "frente", "seguir", paradas);
  const efeito = x.efeito.id;
  const clipeId = x.clipe.id;
  return (
    <div style={{ ...BARRA, top: area.y + 8, left: area.x + area.w / 2 }} data-seguir-palco="">
      <button type="button" style={{ ...BOTAO, opacity: tras ? 1 : 0.4 }} disabled={!tras} onClick={() => void seguirEfeito(efeito, clipeId, "tras")} data-acao="seguir-tras">
        <ChevronsLeft size={13} aria-hidden /> Seguir para trás
      </button>
      <button type="button" style={{ ...BOTAO, opacity: frente ? 1 : 0.4 }} disabled={!frente} onClick={() => void seguirEfeito(efeito, clipeId, "frente")} data-acao="seguir">
        Seguir <ChevronsRight size={13} aria-hidden />
      </button>
    </div>
  );
}
