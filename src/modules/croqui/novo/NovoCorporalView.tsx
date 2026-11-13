/** "Novo croqui corporal": nome e prancha, com a arte real em miniatura e em tamanho grande. */

import { useEffect, useRef, useState } from "react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { selectActiveOccurrence, selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { Plus } from "lucide-react";
import { useCroquiStore } from "../store/croquiStore";
import { BODY_TEMPLATES, type BodyView } from "../corpo/assets/bodyTemplates";
import { makeCorpoDoc } from "../corpo/engine/factories";
import styles from "./NovoCorporalView.module.css";

const PRANCHAS: BodyView[] = ["corpo_masc", "corpo_fem", "corpo_lateral_masc", "corpo_lateral_fem"];

function nomePadrao(existentes: string[]): string {
  let n = 1;
  while (existentes.includes(`Croqui corporal ${n}`)) n++;
  return `Croqui corporal ${n}`;
}

/** Desenha a prancha (com o recorte da arte, quando houver) num canvas que preenche a caixa. */
function Arte({ id, className }: { id: BodyView; className?: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const tpl = BODY_TEMPLATES[id];
    const c = ref.current;
    if (!c) return;
    let vivo = true;
    const im = new Image();
    im.onload = () => {
      if (!vivo) return;
      const r = tpl.crop ?? { x: 0, y: 0, w: im.width, h: im.height };
      c.width = r.w;
      c.height = r.h;
      const cx = c.getContext("2d");
      cx?.drawImage(im, r.x, r.y, r.w, r.h, 0, 0, r.w, r.h);
    };
    im.src = tpl.src;
    return () => {
      vivo = false;
    };
  }, [id]);
  return <canvas ref={ref} className={className} role="img" aria-label={BODY_TEMPLATES[id].label} />;
}

export function NovoCorporalView() {
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const list = useCroquiStore((s) => s.list);
  const createCroqui = useCroquiStore((s) => s.createCroqui);
  const loadList = useCroquiStore((s) => s.loadList);
  const openCorpo = useCroquiStore((s) => s.openCorpo);
  const fechar = useCroquiStore((s) => s.fecharNovoCorporal);

  const [nome, setNome] = useState(() => nomePadrao(list.map((c) => c.title)));
  const [prancha, setPrancha] = useState<BodyView>("corpo_masc");
  const [busy, setBusy] = useState(false);
  const [erro, setErro] = useState<string | null>(null);

  const criar = async () => {
    if (!workspacePath || busy) return;
    setBusy(true);
    setErro(null);
    try {
      const titulo = nome.trim() || nomePadrao(list.map((c) => c.title));
      const created = await createCroqui(workspacePath, titulo, "corporal");
      const doc = makeCorpoDoc(created.id, created.occurrence_id, { template_id: prancha, title: titulo });
      await commands.saveCroqui(workspacePath, created.id, doc);
      await loadList(workspacePath);
      openCorpo(created.id);
      fechar();
    } catch (err) {
      setErro(toSicroError(err).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className={styles.wrap}>
      <header className={styles.header}>
        <button type="button" className={styles.crumb} onClick={fechar}>Croquis</button>
        <span className={styles.crumbSep}>›</span>
        <span className={styles.crumbAtual}>Novo croqui corporal</span>
        {occurrence?.titulo && <span className={styles.caso}>{occurrence.titulo}</span>}
      </header>
      <div className={styles.body}>
        <section className={styles.col} aria-label="Configuração do croqui">
          <label className={styles.campo}>
            <span className={styles.rotulo}>NOME</span>
            <input className={styles.input} value={nome} onChange={(e) => setNome(e.target.value)} disabled={busy} />
          </label>
          <fieldset className={styles.pranchas}>
            <legend className={styles.rotulo}>PRANCHA</legend>
            {PRANCHAS.map((id) => (
              <button
                key={id}
                type="button"
                aria-pressed={prancha === id}
                className={`${styles.prancha} ${prancha === id ? styles.pranchaAtiva : ""}`}
                onClick={() => setPrancha(id)}
                disabled={busy}
              >
                <Arte id={id} className={styles.miniatura} />
                <span className={styles.pranchaNome}>{BODY_TEMPLATES[id].label}</span>
              </button>
            ))}
          </fieldset>
          <div className={styles.espaco} />
          {erro && <p className={styles.erro}>{erro}</p>}
          <div className={styles.rodape}>
            <button type="button" className={styles.btn} onClick={fechar} disabled={busy}>Cancelar</button>
            <button type="button" className={styles.btnPrimario} onClick={() => void criar()} disabled={busy || !workspacePath}>
              <Plus size={16} /> Criar croqui
            </button>
          </div>
        </section>
        <section className={styles.previa} aria-label="Prévia">
          <div className={styles.previaTopo}>
            <span className={styles.rotulo}>PRÉVIA</span>
            <span className={styles.dica}>{BODY_TEMPLATES[prancha].label}</span>
          </div>
          <div className={styles.previaCaixa}>
            <Arte id={prancha} className={styles.previaArte} />
          </div>
        </section>
      </div>
    </div>
  );
}
