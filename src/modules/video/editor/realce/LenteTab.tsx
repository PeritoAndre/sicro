import { useEffect, useRef, useState } from "react";
import { Clapperboard, Save, Trash2, X, Image as ImageIcon } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { PerfilDeVideo } from "@domain/video_realce";
import { LENTE_PADRAO, useRealceStore } from "./realceStore";
import type { CtxRealce } from "./tipos";
import styles from "./Realce.module.css";

const CAMPOS: { k: "k1" | "k2" | "cx" | "cy"; rotulo: string; min: number; max: number; passo: number; dica: string }[] = [
  { k: "k1", rotulo: "k1", min: -0.5, max: 0.5, passo: 0.005, dica: "Distorção principal: negativo desfaz o barril (linhas curvas para fora), positivo desfaz a almofada" },
  { k: "k2", rotulo: "k2", min: -0.3, max: 0.3, passo: 0.005, dica: "Ajuste fino nas bordas" },
  { k: "cx", rotulo: "centro x", min: 0.3, max: 0.7, passo: 0.002, dica: "Centro óptico, horizontal (0,5 = meio)" },
  { k: "cy", rotulo: "centro y", min: 0.3, max: 0.7, passo: 0.002, dica: "Centro óptico, vertical (0,5 = meio)" },
];

export function LenteTab({ workspacePath, media, onVideoDerivado, usarQuadro }: CtxRealce) {
  const s = useRealceStore();
  const [outros, setOutros] = useState<PerfilDeVideo[]>([]);
  const [nome, setNome] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const salvoRef = useRef<string>("");
  const q = s.quadro;
  const t = s.tarefa;
  const pct = t && t.total > 0 ? Math.round((t.feitos / t.total) * 100) : 0;
  const L = s.lente;
  const igual = (a: typeof L) => `${a.k1}|${a.k2}|${a.cx}|${a.cy}`;

  // perfil salvo deste vídeo + perfis de outros vídeos do caso
  useEffect(() => {
    void commands.realceLenteObter(workspacePath, media.sha256).then((p) => {
      if (p) {
        useRealceStore.getState().setLenteCompleta(p);
        setNome(p.nome);
        salvoRef.current = igual(p);
      } else {
        useRealceStore.getState().setLenteCompleta(LENTE_PADRAO);
        salvoRef.current = "";
      }
      useRealceStore.getState().setTemPerfil(p != null);
    });
    void commands.realceLenteListar(workspacePath).then((l) => setOutros(l.filter((x) => x.media_hash !== media.sha256))).catch(() => setOutros([]));
  }, [workspacePath, media.sha256]);

  // prévia ao vivo: espera o controle parar de mexer
  useEffect(() => {
    if (!q) return;
    if (L.k1 === 0 && L.k2 === 0 && L.cx === 0.5 && L.cy === 0.5) {
      s.setLentePrevia(null);
      return;
    }
    let vivo = true;
    const id = setTimeout(() => {
      commands
        .realceLentePrevia(workspacePath, media.sha256, q.t, L, 1280)
        .then((p) => vivo && useRealceStore.getState().setLentePrevia(p))
        .catch((e) => vivo && useRealceStore.getState().setErro(toSicroError(e).message));
    }, 160);
    return () => {
      vivo = false;
      clearTimeout(id);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q?.t, L.k1, L.k2, L.cx, L.cy, workspacePath, media.sha256]);

  const salvar = async () => {
    try {
      await commands.realceLenteSalvar(workspacePath, media.sha256, { ...L, nome: nome.trim() });
      salvoRef.current = igual(L);
      s.setTemPerfil(true);
      setMsg("Perfil salvo para este vídeo.");
      if (q) await usarQuadro(q.t);
    } catch (e) {
      s.setErro(toSicroError(e).message);
    }
  };
  const apagar = async () => {
    await commands.realceLenteApagar(workspacePath, media.sha256).catch(() => undefined);
    s.setTemPerfil(false);
    s.setLenteCompleta(LENTE_PADRAO);
    salvoRef.current = "";
    setMsg("Perfil apagado.");
  };
  const exportarQuadro = async () => {
    if (!q) return;
    const r = await s.executar("trabalho", () => commands.realceQuadroCorrigido(workspacePath, media.sha256, q.t));
    if (r) {
      setMsg("Quadro corrigido gravado nos resultados.");
      void s.recarregar(workspacePath, media.sha256);
    }
  };
  const copiaVideo = async () => {
    const r = await s.executar("lente-video", () => commands.realceLenteVideo(workspacePath, media.sha256));
    if (!r) return;
    void s.recarregar(workspacePath, media.sha256);
    onVideoDerivado(r.media);
  };

  const sujo = igual(L) !== salvoRef.current;
  const salvo = s.temPerfil && !sujo;
  return (
    <section className={styles.sec}>
      {!q && <div className={styles.dim}>Escolha um quadro com linhas que deviam ser retas (poste, calçada, parede).</div>}
      {CAMPOS.map((c) => (
        <div className={styles.slider} key={c.k} title={c.dica}>
          <span>{c.rotulo}</span>
          <input type="range" min={c.min} max={c.max} step={c.passo} value={L[c.k]} onChange={(e) => s.setLente({ [c.k]: Number(e.target.value) })} />
          <code>{L[c.k].toFixed(3).replace(".", ",")}</code>
        </div>
      ))}
      <div className={styles.line}>
        <label className={styles.check}>
          <input type="checkbox" checked={s.grade} onChange={(e) => s.setGrade(e.target.checked)} /> grade de referência
        </label>
        <button type="button" className={styles.link} onClick={() => s.setLenteCompleta(LENTE_PADRAO)}>zerar</button>
      </div>
      {outros.length > 0 && (
        <label className={styles.line}>
          <span className={styles.dim}>Perfil de outro vídeo</span>
          <select
            value=""
            onChange={(e) => {
              const o = outros.find((x) => x.media_hash === e.target.value);
              if (o) {
                s.setLenteCompleta({ ...o.perfil });
                setNome(o.perfil.nome);
              }
            }}
          >
            <option value="">escolher…</option>
            {outros.map((o) => (
              <option key={o.media_hash} value={o.media_hash}>{o.perfil.nome ? `${o.perfil.nome} · ` : ""}{o.filename}</option>
            ))}
          </select>
        </label>
      )}
      <div className={styles.line}>
        <input className={styles.texto} placeholder="Câmera (opcional)" value={nome} onChange={(e) => setNome(e.target.value)} />
        <Button variant="primary" size="sm" leftIcon={<Save size={13} />} onClick={() => void salvar()} disabled={salvo || (L.k1 === 0 && L.k2 === 0 && L.cx === 0.5 && L.cy === 0.5)}>
          Salvar perfil
        </Button>
        {s.temPerfil && (
          <Button variant="ghost" size="sm" leftIcon={<Trash2 size={13} />} onClick={() => void apagar()}>Apagar</Button>
        )}
      </div>
      {s.temPerfil && (
        <>
          <div className={styles.dim}>O perfil vale para este vídeo: dá para aplicá-lo ao quadro de trabalho e ao realce (caixa no alto) e a uma cópia do vídeo.</div>
          <div className={styles.line}>
            <Button variant="secondary" size="sm" leftIcon={<ImageIcon size={13} />} onClick={() => void exportarQuadro()} disabled={!q || s.tarefa != null}>
              Quadro corrigido
            </Button>
            {t?.fase === "lente-video" ? (
              <Button variant="secondary" size="sm" leftIcon={<X size={13} />} onClick={s.cancelar}>Cancelar {pct}%</Button>
            ) : (
              <Button variant="secondary" size="sm" leftIcon={<Clapperboard size={13} />} onClick={() => void copiaVideo()} disabled={s.tarefa != null}>
                Cópia do vídeo
              </Button>
            )}
          </div>
        </>
      )}
      {msg && <div className={styles.dim}>{msg}</div>}
    </section>
  );
}
