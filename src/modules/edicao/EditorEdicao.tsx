/**
 * Tela da Edição, Fases 0 a 3 (§8.2): barra (título, gravação por ícone, Desfazer, Refazer, Fila, Exportar…),
 * Sequência · Palco · Inspetor em colunas recolhíveis com largura guardada, barra de estado (duração, clipes,
 * problemas) e os atalhos do escopo "edicao" mais os do Vídeo reaproveitados (§8.5).
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type PointerEvent as PE } from "react";
import { Check, CircleDot, Crosshair, Loader2, PanelLeftClose, PanelLeftOpen, PanelRightClose, PanelRightOpen, Redo2, TriangleAlert, Undo2 } from "lucide-react";
import { Button } from "@components/Button/Button";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { useMenusAbertos } from "./menus";
import { useShortcuts } from "@core/useShortcuts";
import { useGestoTransporte } from "@modules/video/editor/useGestoTransporte";
import type { Caixa, Lugar, Ponto, Pts, Seg } from "@domain/edicao";
import { useEdicaoStore } from "./store/edicaoStore";
import { useRenderStore } from "./store/renderStore";
import { useFundoStore } from "./store/fundoStore";
import { acharMidia, acharRastreio, porChave } from "./modelo/projeto";
import { resolver } from "./modelo/tempo";
import { avaliar as avaliarRastreio } from "./modelo/rastreio";
import { Sequencia } from "./sequencia/Sequencia";
import { congelar, dividir, duplicar, mover, remover, vizinho } from "./sequencia/acoes";
import { duracaoDoPrograma, textoClipes, textoDuracaoMs, textoProblemas } from "./sequencia/contas";
import { Palco, type PalcoApi } from "./palco/Palco";
import { recursoDe, useRecursos } from "./palco/recursos";
import { Inspetor } from "./inspetor/Inspetor";
import { DialogoExportar } from "./exportar/DialogoExportar";
import { Fila } from "./exportar/Fila";
import { irParaProblema, useExportarUi } from "./exportar/avisos";
import { receberMarca, useInspetorUi } from "./inspetor/ui";
import { efetivas, gradeDasColunas, LIMITES, useColunas } from "./colunas";
import styles from "./EditorEdicao.module.css";
import { capturar } from "./palco/capturar";
// Fase 2 (contratos-fase2.md (c)): rastreio assistido do P2.4 (sem efeito enquanto RASTREIO_PRONTO = false)
import { atalhosDoRastreio, pararCorrida, useOuvirRastreio } from "./store/rastreioStore";

/** Atalhos da tela: só na frente, sem diálogo e sem menu aberto (Fila, menu do cartão, menu de contexto). */
export function atalhosLigados(x: { ativo: boolean; exportando: boolean; modal: boolean; semDialogo: boolean; semMenu: boolean }): boolean {
  return x.ativo && !x.exportando && !x.modal && x.semDialogo && x.semMenu;
}

function IconeGravacao({ estado }: { estado: "salvo" | "alterado" | "salvando" | "erro" }) {
  const r = { salvo: "Salvo", alterado: "Alterado", salvando: "Salvando…", erro: "Não salvou" }[estado];
  const I = { salvo: Check, alterado: CircleDot, salvando: Loader2, erro: TriangleAlert }[estado];
  return (
    <span className={`${styles.gravacao} ${styles[estado] ?? ""}`} title={r} aria-label={r} role="status" data-gravacao={estado}>
      <I size={15} className={estado === "salvando" ? styles.gira : undefined} aria-hidden />
    </span>
  );
}

/** `ativo` falso: escondida atrás da análise (§8.1); sem atalhos. */
export function EditorEdicao({ ativo = true }: { ativo?: boolean } = {}) {
  const doc = useEdicaoStore((s) => s.doc);
  const salvando = useEdicaoStore((s) => s.salvando);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  const passado = useEdicaoStore((s) => s.passado.length);
  const futuro = useEdicaoStore((s) => s.futuro.length);
  const analise = useEdicaoStore((s) => s.analise);
  const avisos = useEdicaoStore((s) => s.avisos);
  const versao = useRecursos((s) => s.versao);
  const [colunas, setColunas] = useColunas();
  const [expandido, setExpandido] = useState(false);
  const ferramenta = useInspetorUi((s) => s.ferramenta);
  const exportando = useExportarUi((s) => s.dialogo);
  const [mudo, setMudo] = useState(false);
  /** Diálogo da Sequência aberto: os atalhos da tela ficam desligados (Enter, Del e setas são do diálogo). */
  const [modal, setModal] = useState(false);
  // qualquer diálogo aberto (Sequência, Inspetor, confirmações, exportação) desliga os atalhos da tela (h 25)
  const semDialogo = useDialogosAbertos((s) => s.pilha.length === 0);
  const semMenu = useMenusAbertos((s) => s.n === 0);
  const atalhos = atalhosLigados({ ativo, exportando, modal, semDialogo, semMenu });
  useOuvirRastreio();
  const palco = useRef<PalcoApi | null>(null);
  // largura do corpo: as laterais cedem quando o palco ficaria estreito
  const corpoRef = useRef<HTMLDivElement | null>(null);
  const [disponivel, setDisponivel] = useState(0);
  useLayoutEffect(() => {
    const el = corpoRef.current;
    if (!el) return;
    const medir = () => setDisponivel(el.clientWidth);
    medir();
    const ro = new ResizeObserver(medir);
    ro.observe(el);
    return () => ro.disconnect();
  }, [doc !== null]); // eslint-disable-line react-hooks/exhaustive-deps

  // filas: eventos assinados uma vez
  useEffect(() => {
    void useRenderStore.getState().ouvir().catch(() => {});
    void useFundoStore.getState().ouvir().catch(() => {});
  }, []);

  // ---- linha resolvida pela tela (sem esperar o Rust) ----
  const linha = useMemo(() => (doc ? resolver(doc, { tabela: (id) => recursoDe(acharMidia(doc, id)?.sha256).tabela }) : null), [doc, versao]); // eslint-disable-line react-hooks/exhaustive-deps
  const nClipes = linha?.clipes.length ?? 0;
  const problemas = useMemo(() => [...avisos, ...(analise?.problemas ?? [])], [avisos, analise]);
  const contagem = textoProblemas(problemas);

  // ---- marcação no palco ----
  const onMarcado = useCallback((v: Caixa | Ponto | Lugar, t: Seg, pts: Pts | null) => receberMarca(v, t, pts), []);

  /** Enter: âncora no quadro mostrado com a caixa que o rastreio dá ali (interpolada ou segurada). */
  const marcarNesteQuadro = useCallback(() => {
    const st = useEdicaoStore.getState();
    const f = ferramenta;
    const inst = f?.rastreio ? palco.current?.instante(f.rastreio) : null;
    if (!f?.rastreio || !st.doc || !inst) return;
    const r = acharRastreio(st.doc, f.rastreio);
    const c = r ? avaliarRastreio(r, inst) : null;
    if (!r || !c) return;
    st.alterar("Marcar neste quadro", (d) => {
      const rr = acharRastreio(d, r.id);
      if (rr) rr.ancoras = porChave(rr.ancoras, { t: inst.t, pts: inst.pts, v: c.caixa });
    });
  }, [ferramenta]);

  // ---- atalhos (§8.5) ----
  const abrirExportar = useCallback(() => {
    if (!soLeitura) useExportarUi.getState().abrirExportar();
  }, [soLeitura]);
  useShortcuts(
    {
      "edicao.desfazer": () => useEdicaoStore.getState().desfazer(),
      "edicao.refazer": () => useEdicaoStore.getState().refazer(),
      "edicao.refazerAlt": () => useEdicaoStore.getState().refazer(),
      "edicao.salvar": () => void useEdicaoStore.getState().salvar().catch(() => {}),
      "edicao.exportar": abrirExportar,
      "edicao.clipeAnterior": () => vizinho(-1),
      "edicao.proximoClipe": () => vizinho(1),
      "edicao.subir": () => mover(-1),
      "edicao.descer": () => mover(1),
      // tocando, o cursor da store anda a cada 250 ms: o palco para antes, no quadro mostrado
      "edicao.dividir": () => {
        palco.current?.pausar();
        dividir();
      },
      "edicao.congelar": () => {
        palco.current?.pausar();
        congelar();
      },
      "edicao.remover": () => remover(),
      "edicao.duplicar": () => duplicar(),
      "edicao.quadroFinal": () => palco.current?.quadroFinal(),
      // Enter só é da marcação com a ferramenta de rastreio ligada; fora disso segue para o botão em foco
      ...(ferramenta?.rastreio ? { "edicao.marcar": marcarNesteQuadro } : {}),
      ...atalhosDoRastreio(),
      "edicao.cancelar": () => {
        if (pararCorrida()) return;
        if (palco.current?.cancelar()) return;
        if (ferramenta) useInspetorUi.getState().cancelarMarcacao();
        else if (expandido) setExpandido(false);
      },
      "video.playPause": () => palco.current?.tocarPausar(),
      "video.playPauseK": () => palco.current?.tocarPausar(),
      "video.prevFrame": () => palco.current?.passo(-1),
      "video.nextFrame": () => palco.current?.passo(1),
      "video.seekStart": () => palco.current?.inicio(),
      "video.seekEnd": () => palco.current?.fim(),
      "video.markIn": () => palco.current?.entrada(),
      "video.markOut": () => palco.current?.saida(),
      "video.magnifyIn": () => palco.current?.zoom(1.25),
      "video.magnifyOut": () => palco.current?.zoom(0.8),
      "video.magnifyReset": () => palco.current?.zoom("ajustar"),
      "video.fullscreen": () => setExpandido((x) => !x),
      "video.mute": () => setMudo((x) => !x),
    },
    { enabled: atalhos },
  );
  // escondida: para de tocar (clipe sem fonte toca pelo relógio; não há <video> para o VideoModule pausar)
  useEffect(() => {
    if (!ativo) palco.current?.pausar();
  }, [ativo]);
  // ←/→ (toque = um quadro; segurar toca ou dá ré) e Shift+←/→ (±1 s): o gesto do reprodutor da análise (§8.5)
  useGestoTransporte(
    {
      passo: (d) => palco.current?.passo(d),
      mover: (d) => palco.current?.tocar(d),
      parar: () => palco.current?.pausar(),
      saltar: (s) => {
        const p = palco.current;
        if (p) p.irPara(p.tempo() + s);
      },
    },
    atalhos,
  );

  // ---- arrastar a divisa das colunas ----
  const arrastarDivisa = (qual: "seq" | "insp") => (e: PE<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const alvo = e.currentTarget;
    capturar(alvo, e.pointerId);
    const x0 = e.clientX;
    const w0 = efetivas(colunas, disponivel)[qual];
    const [a, b] = LIMITES[qual];
    const mover = (ev: PointerEvent) => {
      const d = qual === "seq" ? ev.clientX - x0 : x0 - ev.clientX;
      const w = Math.min(Math.max(w0 + d, a), b);
      setColunas((c) => ({ ...c, [qual]: w }));
    };
    const soltar = () => {
      alvo.removeEventListener("pointermove", mover);
      alvo.removeEventListener("pointerup", soltar);
      alvo.removeEventListener("pointercancel", soltar);
    };
    alvo.addEventListener("pointermove", mover);
    alvo.addEventListener("pointerup", soltar);
    alvo.addEventListener("pointercancel", soltar);
  };

  /** Barra de estado: leva ao primeiro item com problema. */
  const irAoProblema = () => {
    const p = problemas.find((x) => x.nivel === "erro") ?? problemas[0];
    if (p) irParaProblema(p);
  };

  if (!doc) return null;
  const st = useEdicaoStore.getState;
  return (
    <div className={styles.editor} data-editor="">
      <header className={styles.topo}>
        <button type="button" className={styles.recolher} title="Sequência" aria-label="Sequência" aria-pressed={!colunas.seqFechada} onClick={() => setColunas((c) => ({ ...c, seqFechada: !c.seqFechada }))}>
          {colunas.seqFechada ? <PanelLeftOpen size={15} /> : <PanelLeftClose size={15} />}
        </button>
        <input
          className={styles.titulo}
          value={doc.titulo}
          aria-label="Título"
          readOnly={soLeitura}
          onChange={(e) => {
            const v = e.target.value;
            st().alterar("Título", (d) => {
              d.titulo = v;
            }, { agrupar: "titulo" });
          }}
        />
        <IconeGravacao estado={salvando} />
        <span className={styles.sep} />
        <Button variant="ghost" size="sm" leftIcon={<Undo2 size={14} />} disabled={soLeitura || passado === 0} onClick={() => st().desfazer()}>
          Desfazer
        </Button>
        <Button variant="ghost" size="sm" leftIcon={<Redo2 size={14} />} disabled={soLeitura || futuro === 0} onClick={() => st().refazer()}>
          Refazer
        </Button>
        <span className={styles.espaco} />
        <Fila />
        <Button variant="primary" size="sm" disabled={soLeitura} onClick={abrirExportar}>
          Exportar…
        </Button>
        <button type="button" className={styles.recolher} title="Inspetor" aria-label="Inspetor" aria-pressed={!colunas.inspFechada} onClick={() => setColunas((c) => ({ ...c, inspFechada: !c.inspFechada }))}>
          {colunas.inspFechada ? <PanelRightOpen size={15} /> : <PanelRightClose size={15} />}
        </button>
      </header>
      <div ref={corpoRef} className={styles.corpo} style={{ gridTemplateColumns: gradeDasColunas(colunas, expandido, disponivel) }}>
        <aside className={`${styles.coluna} ${styles.esquerda}`} hidden={expandido || colunas.seqFechada} data-coluna="sequencia">
          <Sequencia onModal={setModal} pararPalco={() => palco.current?.pausar()} />
        </aside>
        <div className={styles.divisa} onPointerDown={colunas.seqFechada ? undefined : arrastarDivisa("seq")} hidden={expandido || colunas.seqFechada} />
        <main className={styles.centro} data-coluna="palco">
          <Palco
            ref={palco}
            modo="clipe"
            ferramenta={ferramenta}
            onMarcado={onMarcado}
            expandido={expandido}
            onExpandir={() => setExpandido((x) => !x)}
            mudo={mudo}
          />
        </main>
        <div className={styles.divisa} onPointerDown={colunas.inspFechada ? undefined : arrastarDivisa("insp")} hidden={expandido || colunas.inspFechada} />
        <aside className={`${styles.coluna} ${styles.direita}`} hidden={expandido || colunas.inspFechada} data-coluna="inspetor">
          <Inspetor />
        </aside>
      </div>
      <footer className={styles.estado} data-estado-barra="">
        <span className={styles.mono}>{textoDuracaoMs(duracaoDoPrograma(linha))}</span>
        <span className={styles.ponto}>·</span>
        <span>{textoClipes(nClipes)}</span>
        {contagem && (
          <>
            <span className={styles.ponto}>·</span>
            <button type="button" className={`${styles.problemas} ${contagem.erro ? styles.erro : styles.aviso}`} onClick={irAoProblema}>
              {contagem.texto}
            </button>
          </>
        )}
        {ferramenta && (
          <button type="button" className={styles.ferramenta} title="Parar" aria-label="Parar" onClick={() => useInspetorUi.getState().cancelarMarcacao()} data-ferramenta={ferramenta.tipo}>
            <Crosshair size={14} aria-hidden />
          </button>
        )}
      </footer>
      <DialogoExportar />
    </div>
  );
}
