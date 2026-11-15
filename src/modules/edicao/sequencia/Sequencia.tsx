/**
 * Sequência (§8.2, D24): vista permanente da trilha principal. Botões de item novo, um cartão por item (os de versão
 * futura e os inválidos apagados), arrastar pela alça para reordenar (pointer events: o arrastar HTML5 briga com o
 * dragDrop do Tauri no Windows). O que está preso a cada clipe é do `Presos`, dentro do cartão.
 */

import { useEffect, useMemo, useRef, useState, type PointerEvent as PE, type ReactNode, type RefObject } from "react";
import { CalendarRange, Film, Image as IconeImagem, Snowflake, Type } from "lucide-react";
import { open as abrirArquivo } from "@tauri-apps/plugin-dialog";
import { commands } from "@core/commands";
import type { Analise, ClipePrincipal, Id, LinhaResolvida, Problema, ProjetoEdicao, Titulo } from "@domain/edicao";
import { pushToast } from "@components/toast/toastStore";
import { separarErro, useEdicaoStore } from "../store/edicaoStore";
import { acharMidia, garantirMidia } from "../modelo/projeto";
import { resolver } from "../modelo/tempo";
import { garantirMidia as pedirMidia, recursoDe, useRecursos, type EstadoRecurso } from "../palco/recursos";
import { CartaoClipe } from "./CartaoClipe";
import { NovoClipe, type Formulario } from "./NovoClipe";
import { clipeNaTela, destinoDoArrasto, duracaoNaLinha, problemasDoItem, rotuloDoTipo, selosDoClipe } from "./contas";
import { novoClipeImagem, novoClipeTitulo } from "./novos";
import { congelar, duplicar, inserir, inserirTrechos, moverPara, podeCongelar, remover, selecionarClipe } from "./acoes";
import styles from "./Sequencia.module.css";
import { capturar } from "../palco/capturar";
import { useOrigemStore, type OrigemDoClipe } from "../store/origemStore";
import { useOrigensDosClipes } from "./origens";
// as listas que o Rust aceita (`armazem::{EXT_IMAGEM, EXT_AUDIO}`): o diálogo não oferece o que a importação recusa
import { EXTENSOES } from "../inspetor/secoes/importar";

/** Nome mostrado: o "Nome" do clipe, senão o arquivo da mídia sem extensão. */
function nomeDo(c: ClipePrincipal, midias: { id: Id; nome: string }[]): string {
  if (c.rotulo) return c.rotulo;
  if (c.tipo === "trecho" || c.tipo === "congelar" || c.tipo === "imagem") {
    const m = midias.find((x) => x.id === c.midia);
    return m ? m.nome.replace(/\.[^.]+$/, "") : "";
  }
  if (c.tipo === "titulo") return c.titulo.blocos[0]?.conteudo ?? "";
  return "";
}

export function Sequencia({
  onModal,
  pararPalco,
}: {
  /** Formulário (diálogo) aberto ou fechado. */
  onModal?(aberto: boolean): void;
  /** Para o palco que toca: o cursor da store vai ao quadro mostrado antes de "Congelar aqui". */
  pararPalco?(): void;
}) {
  const doc = useEdicaoStore((s) => s.doc);
  const ws = useEdicaoStore((s) => s.ws);
  const analise = useEdicaoStore((s) => s.analise);
  const avisos = useEdicaoStore((s) => s.avisos);
  const selecao = useEdicaoStore((s) => s.selecao);
  const cursor = useEdicaoStore((s) => s.cursor);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  const versao = useRecursos((s) => s.versao);
  const origens = useOrigemStore((s) => s.porClipe);
  useOrigensDosClipes();
  const [form, setForm] = useState<Formulario | null>(null);
  const [arrasto, setArrasto] = useState<{ de: number; para: number } | null>(null);
  const listaRef = useRef<HTMLOListElement | null>(null);
  const aberto = form !== null;
  useEffect(() => {
    onModal?.(aberto);
    return () => {
      if (aberto) onModal?.(false);
    };
  }, [aberto]); // eslint-disable-line react-hooks/exhaustive-deps

  // todas as mídias do projeto: tabela (duração local) e selos
  useEffect(() => {
    if (!ws || !doc) return;
    for (const m of doc.midias) if (m.tipo === "video") pedirMidia(ws, m.sha256);
  }, [ws, doc?.midias]); // eslint-disable-line react-hooks/exhaustive-deps

  const linha = useMemo(() => (doc ? resolver(doc, { tabela: (id) => recursoDe(acharMidia(doc, id)?.sha256).tabela }) : null), [doc, versao]); // eslint-disable-line react-hooks/exhaustive-deps
  const podeCong = useMemo(() => podeCongelar(), [doc, cursor, versao]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!doc || !ws) return null;
  const focoId = selecao.foco?.k === "clipe" ? selecao.foco.id : cursor.modo === "clipe" ? cursor.clipe : null;

  const comecarArrasto = (i: number) => (e: PE<HTMLElement>) => {
    if (e.button !== 0 || soLeitura) return;
    e.preventDefault();
    e.stopPropagation();
    const alvo = e.currentTarget;
    capturar(alvo, e.pointerId);
    const centros = () => [...(listaRef.current?.querySelectorAll<HTMLElement>("[data-cartao]") ?? [])].map((el) => {
      const r = el.getBoundingClientRect();
      return r.top + r.height / 2;
    });
    const c0 = centros();
    setArrasto({ de: i, para: i });
    const mover = (ev: PointerEvent) => setArrasto({ de: i, para: destinoDoArrasto(ev.clientY, c0, i) });
    const soltar = (ev: PointerEvent) => {
      alvo.removeEventListener("pointermove", mover);
      alvo.removeEventListener("pointerup", soltar);
      alvo.removeEventListener("pointercancel", soltar);
      setArrasto(null);
      if (ev.type === "pointerup") moverPara(i, destinoDoArrasto(ev.clientY, c0, i));
    };
    alvo.addEventListener("pointermove", mover);
    alvo.addEventListener("pointerup", soltar);
    alvo.addEventListener("pointercancel", soltar);
  };

  const importar = async (tipo: "imagem" | "audio"): Promise<{ id: Id; nome: string } | null> => {
    const st = useEdicaoStore.getState();
    if (!st.doc) return null;
    const escolhido = await abrirArquivo({ multiple: false, filters: [{ name: tipo === "imagem" ? "Imagem" : "Narração", extensions: EXTENSOES[tipo] }] });
    if (typeof escolhido !== "string") return null;
    // a pasta do projeto precisa existir antes da mídia própria
    await st.descarregar().catch(() => {});
    const ref = await commands.edicaoMidiaImportar(ws, st.doc.id, escolhido, tipo);
    let id = ref.id;
    useEdicaoStore.getState().alterar(tipo === "imagem" ? "Imagem" : "Narração", (d) => {
      id = garantirMidia(d, ref);
    });
    return { id, nome: ref.nome };
  };

  /** Falha de importação (arquivo vazio, extensão, pasta sem gravação): a frase do catálogo, no aviso do app. */
  const falhou = (e: unknown) => {
    const r = separarErro(e);
    pushToast("error", r.frase || r.codigo || "");
    return null;
  };

  const novaImagem = async () => {
    const m = await importar("imagem").catch(falhou);
    if (m) inserir("Imagem", () => [novoClipeImagem(m.id)]);
  };

  const novoTitulo = (t: Titulo) => {
    setForm(null);
    inserir("Título", () => [novoClipeTitulo(t)]);
  };

  return (
    <VistaSequencia
      doc={doc}
      analise={analise}
      avisos={avisos}
      focoId={focoId}
      soLeitura={soLeitura}
      linha={linha}
      podeCongelar={podeCong}
      recurso={recursoDe}
      arrasto={arrasto}
      origens={origens}
      listaRef={listaRef}
      onForm={setForm}
      onCongelar={() => {
        pararPalco?.();
        congelar();
      }}
      onImagem={() => void novaImagem()}
      onSelecionar={selecionarClipe}
      onDuplicar={duplicar}
      onRemover={remover}
      onAlca={comecarArrasto}
    >
      <NovoClipe
        qual={form}
        ws={ws}
        onFechar={() => setForm(null)}
        onTrechos={(t) => {
          setForm(null);
          inserirTrechos(t);
        }}
        onTitulo={novoTitulo}
      />
    </VistaSequencia>
  );
}

// CONTRATO: saem `onAdicionar` e `onPreso`; entra `origens?` (contratos-fase1 F8).
export interface VistaSequenciaProps {
  doc: ProjetoEdicao;
  analise: Analise | null;
  avisos: readonly Problema[];
  focoId: Id | null;
  soLeitura: boolean;
  linha: LinhaResolvida | null;
  podeCongelar: boolean;
  recurso(sha: string | undefined): EstadoRecurso;
  arrasto: { de: number; para: number } | null;
  /** Intermediário de origem por clipe (progresso no cartão). */
  origens?: Record<Id, OrigemDoClipe | undefined>;
  listaRef?: RefObject<HTMLOListElement>;
  onForm(f: Formulario): void;
  onCongelar(): void;
  onImagem(): void;
  onSelecionar(id: Id | null): void;
  onDuplicar(id: Id | null): void;
  onRemover(id: Id | null): void;
  onAlca(i: number): (e: PE<HTMLElement>) => void;
  children?: ReactNode;
}

/** A lista sem a store (o que a tela mostra a partir dos dados). */
export function VistaSequencia(p: VistaSequenciaProps) {
  const { doc, analise, avisos, focoId, soLeitura, linha, arrasto } = p;
  return (
    <div className={styles.sequencia} data-sequencia="">
      <div className={styles.barra}>
        <button type="button" className={styles.mini} onClick={() => p.onForm("trecho")} disabled={soLeitura}>
          <Film size={13} aria-hidden />
          Trecho…
        </button>
        <button type="button" className={styles.mini} onClick={() => p.onForm("titulo")} disabled={soLeitura}>
          <Type size={13} aria-hidden />
          Título…
        </button>
        <button type="button" className={styles.mini} onClick={p.onCongelar} disabled={soLeitura || !p.podeCongelar}>
          <Snowflake size={13} aria-hidden />
          Congelar aqui
        </button>
        <button type="button" className={styles.mini} onClick={p.onImagem} disabled={soLeitura}>
          <IconeImagem size={13} aria-hidden />
          Imagem…
        </button>
        <button type="button" className={styles.mini} onClick={() => p.onForm("eventos")} disabled={soLeitura}>
          <CalendarRange size={13} aria-hidden />
          Dos eventos…
        </button>
      </div>
      <ol ref={p.listaRef} className={styles.lista}>
        {doc.principal.map((c, i) => {
          const id = (c as { id?: Id }).id;
          const conhecido = clipeNaTela(c, analise, avisos);
          const probs = id ? problemasDoItem(id, analise, avisos) : [];
          const ref = conhecido && (c.tipo === "trecho" || c.tipo === "congelar") ? acharMidia(doc, c.midia) : null;
          const rec = p.recurso(ref?.sha256);
          const estado = ref ? (analise?.midias.find((m) => m.id === ref.id) ?? null) : null;
          const soltar = arrasto && arrasto.para === i && arrasto.de !== i ? (arrasto.para > arrasto.de ? "depois" : "antes") : null;
          return (
            <CartaoClipe
              key={id ?? `i${i}`}
              id={id ?? undefined}
              indice={i}
              tipo={conhecido ? rotuloDoTipo(c) : null}
              tipoCru={String((c as { tipo?: unknown }).tipo ?? "")}
              nome={conhecido ? nomeDo(c, doc.midias) : ""}
              duracao={conhecido && id ? duracaoNaLinha(id, linha) : null}
              selos={conhecido ? selosDoClipe(c, ref, rec.info, estado, probs) : []}
              problemas={probs}
              selecionado={!!id && id === focoId}
              desligado={conhecido && c.desligado}
              arrastando={arrasto?.de === i}
              soltar={soltar}
              soLeitura={soLeitura}
              origem={conhecido && id ? p.origens?.[id] : undefined}
              onSelecionar={() => p.onSelecionar(id ?? null)}
              onVer={() => p.onSelecionar(id ?? null)}
              onDuplicar={() => p.onDuplicar(id ?? null)}
              onRemover={() => p.onRemover(id ?? null)}
              onAlca={p.onAlca(i)}
            />
          );
        })}
      </ol>
      {p.children}
    </div>
  );
}
