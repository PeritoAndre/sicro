/** Super-resolução (Real-ESRGAN) da imagem ou da seleção: gera um arquivo novo, o original não muda. */
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { FolderOpen, FilePlus2, Sparkles } from "lucide-react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { revealExported } from "@core/reveal";
import type { ResultadoSuperResImagem, Roi } from "@domain/video_realce";
import type { SicroImageSelection } from "../engine/schema";
import { useImagemStore } from "../store/imagemStore";
import styles from "./FilterBench.module.css";

function caixa(sel: SicroImageSelection | null): Roi | null {
  if (!sel) return null;
  if (sel.points && sel.points.length > 2) {
    const xs = sel.points.map((p) => p.x);
    const ys = sel.points.map((p) => p.y);
    const x = Math.floor(Math.min(...xs));
    const y = Math.floor(Math.min(...ys));
    return { x, y, w: Math.ceil(Math.max(...xs)) - x, h: Math.ceil(Math.max(...ys)) - y };
  }
  if (sel.width && sel.height) return { x: Math.round(sel.x ?? 0), y: Math.round(sel.y ?? 0), w: Math.round(sel.width), h: Math.round(sel.height) };
  return null;
}

export function SuperResBox({
  workspacePath,
  relativePath,
  selection,
}: {
  workspacePath: string;
  relativePath: string;
  selection: SicroImageSelection | null;
}) {
  const navigate = useNavigate();
  const createFromFile = useImagemStore((s) => s.createFromFile);
  const [pronto, setPronto] = useState<boolean | null>(null);
  const [escala, setEscala] = useState<2 | 4>(2);
  const [soSelecao, setSoSelecao] = useState(true);
  const [rodando, setRodando] = useState(false);
  const [erro, setErro] = useState<string | null>(null);
  const [res, setRes] = useState<ResultadoSuperResImagem | null>(null);
  const [adicionada, setAdicionada] = useState(false);
  const roi = caixa(selection);

  useEffect(() => {
    void commands.getAiStatus().then((a) => setPronto(a.superres_ok)).catch(() => setPronto(false));
  }, []);

  const rodar = async () => {
    setRodando(true);
    setErro(null);
    setRes(null);
    setAdicionada(false);
    try {
      setRes(await commands.realceSuperresImagem(workspacePath, { relative_path: relativePath, roi: soSelecao ? roi : null, escala }));
    } catch (e) {
      setErro(toSicroError(e).message);
    } finally {
      setRodando(false);
    }
  };

  return (
    <section className={styles.card}>
      <div className={styles.cardHead}>
        <span className={styles.idx}><Sparkles size={11} aria-hidden /></span>
        <span className={styles.cardTitle}>Super-resolução</span>
      </div>
      <div className={styles.cardBody}>
        {pronto === false ? (
          <button type="button" className={styles.add} onClick={() => navigate("/configuracoes?aba=iaocr")}>
            Baixar no Gerenciador de IA
          </button>
        ) : (
          <>
            <div className={styles.scope}>
              <button type="button" className={escala === 2 ? styles.scopeOn : undefined} onClick={() => setEscala(2)}>2x</button>
              <button type="button" className={escala === 4 ? styles.scopeOn : undefined} onClick={() => setEscala(4)}>4x</button>
            </div>
            <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 11 }}>
              <input type="checkbox" checked={soSelecao && roi != null} disabled={roi == null} onChange={(e) => setSoSelecao(e.target.checked)} />
              {roi ? `só a seleção (${roi.w}×${roi.h})` : "sem seleção: a imagem inteira"}
            </label>
            <button type="button" className={styles.add} disabled={rodando || pronto == null} onClick={() => void rodar()}>
              <Sparkles size={14} /> {rodando ? "Ampliando…" : `Ampliar ${escala}x`}
            </button>
            <p className={styles.order}>Realce por rede neural: pode inventar detalhe; não usar para identificação sem o original ao lado.</p>
          </>
        )}
        {erro && <p className={styles.empty} style={{ color: "var(--sicro-danger, #e5484d)" }}>{erro}</p>}
        {res && (
          <p className={styles.order}>
            {res.largura}×{res.altura} px em {res.tempo_s.toFixed(1).replace(".", ",")} s ·{" "}
            <button type="button" className={styles.recipeSave} onClick={() => revealExported(workspacePath, res.relative_path)}>
              <FolderOpen size={11} aria-hidden /> pasta
            </button>{" "}
            <button
              type="button"
              className={styles.recipeSave}
              disabled={adicionada}
              onClick={() =>
                void createFromFile(workspacePath, { source_path: res.absoluto, title: "Super-resolução" })
                  .then(() => setAdicionada(true))
                  .catch((e) => setErro(toSicroError(e).message))
              }
            >
              <FilePlus2 size={11} aria-hidden /> {adicionada ? "na lista de imagens" : "abrir como imagem do caso"}
            </button>
          </p>
        )}
      </div>
    </section>
  );
}
