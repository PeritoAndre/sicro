/** "Novo croqui de planta": nome, papel, orientação e escala; a prévia mostra a folha em metros. */

import { useEffect, useRef, useState } from "react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { selectActiveOccurrence, selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { Plus } from "lucide-react";
import { useCroquiStore } from "../store/croquiStore";
import { emptyPlanta, sheetSize } from "../planta/model/schema";
import styles from "./NovoPlantaView.module.css";

type Papel = "A4" | "A3";
type Orient = "paisagem" | "retrato";
const ESCALAS = [50, 100, 200];

function nomePadrao(existentes: string[]): string {
  let n = 1;
  while (existentes.includes(`Planta ${n}`)) n++;
  return `Planta ${n}`;
}

const fmt = (v: number) => v.toFixed(2).replace(".", ",");

function Previa({ w, h }: { w: number; h: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 600, h: 520 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => e && setBox({ w: Math.max(1, e.contentRect.width), h: Math.max(1, e.contentRect.height) }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const m = 56;
  const s = Math.max(1, Math.min((box.w - 2 * m) / w, (box.h - 2 * m) / h));
  const W = w * s;
  const H = h * s;
  const ox = (box.w - W) / 2;
  const oy = (box.h - H) / 2;
  const passo = w > 30 ? 5 : 1;
  const xs: number[] = [];
  const ys: number[] = [];
  for (let x = 0; x <= w + 1e-6; x += passo) xs.push(x);
  for (let y = 0; y <= h + 1e-6; y += passo) ys.push(y);
  const cota = "var(--sicro-fg-dim)";
  return (
    // o svg fica fora do fluxo: medir a caixa pelo conteúdo fazia ela crescer a cada medida (a folha "caía")
    <div ref={ref} className={styles.previaPalco}>
      <svg width={box.w} height={box.h} role="img" aria-label="Folha da planta">
        <rect x={ox} y={oy} width={W} height={H} fill="#f5f6f8" />
        {xs.map((x) => <line key={`x${x}`} x1={ox + x * s} y1={oy} x2={ox + x * s} y2={oy + H} stroke={x % (passo * 5) === 0 ? "#b8c0cc" : "#dde3ea"} />)}
        {ys.map((y) => <line key={`y${y}`} x1={ox} y1={oy + y * s} x2={ox + W} y2={oy + y * s} stroke={y % (passo * 5) === 0 ? "#b8c0cc" : "#dde3ea"} />)}
        <rect x={ox} y={oy} width={W} height={H} fill="none" stroke="var(--sicro-accent)" strokeWidth={2} />
        <path d={`M${ox} ${oy - 14}v-8M${ox + W} ${oy - 14}v-8M${ox} ${oy - 18}H${ox + W}`} stroke={cota} fill="none" />
        <text x={ox + W / 2} y={oy - 28} textAnchor="middle" fontSize={12} fontFamily="monospace" fill={cota}>{fmt(w)} m</text>
        <path d={`M${ox - 14} ${oy}h-8M${ox - 14} ${oy + H}h-8M${ox - 18} ${oy}V${oy + H}`} stroke={cota} fill="none" />
        <text x={ox - 30} y={oy + H / 2} textAnchor="middle" fontSize={12} fontFamily="monospace" fill={cota} transform={`rotate(-90 ${ox - 30} ${oy + H / 2})`}>{fmt(h)} m</text>
      </svg>
    </div>
  );
}

export function NovoPlantaView() {
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const list = useCroquiStore((s) => s.list);
  const createCroqui = useCroquiStore((s) => s.createCroqui);
  const loadList = useCroquiStore((s) => s.loadList);
  const openPlanta = useCroquiStore((s) => s.openPlanta);
  const fechar = useCroquiStore((s) => s.fecharNovoPlanta);

  const [nome, setNome] = useState(() => nomePadrao(list.map((c) => c.title)));
  const [papel, setPapel] = useState<Papel>("A4");
  const [orient, setOrient] = useState<Orient>("paisagem");
  const [escala, setEscala] = useState(50);
  const [busy, setBusy] = useState(false);
  const [erro, setErro] = useState<string | null>(null);

  const { w, h } = sheetSize(papel, orient, escala);

  const criar = async () => {
    if (!workspacePath || busy) return;
    setBusy(true);
    setErro(null);
    try {
      const titulo = nome.trim() || nomePadrao(list.map((c) => c.title));
      const created = await createCroqui(workspacePath, titulo, "planta");
      const base = emptyPlanta({ planta_id: created.id, occurrence_id: created.occurrence_id, title: titulo });
      const doc = { ...base, sheet: { ...base.sheet, paper: papel, orientation: orient, scale: escala, w, h } };
      await commands.saveCroqui(workspacePath, created.id, doc);
      await loadList(workspacePath);
      openPlanta(created.id);
      fechar();
    } catch (err) {
      setErro(toSicroError(err).message);
    } finally {
      setBusy(false);
    }
  };

  const chip = (ativo: boolean, rotulo: string, onClick: () => void) => (
    <button key={rotulo} type="button" aria-pressed={ativo} className={`${styles.chip} ${ativo ? styles.chipAtivo : ""}`} onClick={onClick} disabled={busy}>
      {rotulo}
    </button>
  );

  return (
    <div className={styles.wrap}>
      <header className={styles.header}>
        <button type="button" className={styles.crumb} onClick={fechar}>Croquis</button>
        <span className={styles.crumbSep}>›</span>
        <span className={styles.crumbAtual}>Novo croqui de planta</span>
        {occurrence?.titulo && <span className={styles.caso}>{occurrence.titulo}</span>}
      </header>
      <div className={styles.body}>
        <section className={styles.col} aria-label="Configuração do croqui">
          <label className={styles.campo}>
            <span className={styles.rotulo}>NOME</span>
            <input className={styles.input} value={nome} onChange={(e) => setNome(e.target.value)} disabled={busy} />
          </label>
          <div className={styles.bloco}>
            <span className={styles.rotulo}>PAPEL</span>
            <div className={styles.chips}>{(["A4", "A3"] as Papel[]).map((p) => chip(papel === p, p, () => setPapel(p)))}</div>
          </div>
          <div className={styles.bloco}>
            <span className={styles.rotulo}>ORIENTAÇÃO</span>
            <div className={styles.chips}>
              {chip(orient === "paisagem", "Paisagem", () => setOrient("paisagem"))}
              {chip(orient === "retrato", "Retrato", () => setOrient("retrato"))}
            </div>
          </div>
          <div className={styles.bloco}>
            <span className={styles.rotulo}>ESCALA DE REFERÊNCIA</span>
            <div className={styles.chips}>{ESCALAS.map((e) => chip(escala === e, `1:${e}`, () => setEscala(e)))}</div>
            <span className={styles.mono}>Folha de {fmt(w)} × {fmt(h)} m</span>
          </div>
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
            <span className={styles.dica}>{papel} · 1:{escala}</span>
          </div>
          <div className={styles.previaCaixa}>
            <Previa w={w} h={h} />
          </div>
        </section>
      </div>
    </div>
  );
}
