/**
 * Diálogo Exportar (§8.6): predefinição, "Arquivo reprodutível", legendas, pacote do júri e revisão, e os problemas
 * da análise atual. Erro bloqueia e leva ao item; aviso pericial pede "Ciente"; o botão diz quantos avisos vão junto.
 * Sem props: abre por `abrirExportar()` e lê tudo da store.
 */

import { useEffect, useState } from "react";
import { AlertTriangle, ChevronRight, Cpu, Loader2, OctagonX, RefreshCw, ShieldAlert, Zap } from "lucide-react";
import { Button } from "@components/Button/Button";
import { Dialog } from "@components/Dialog/Dialog";
import { commands } from "@core/commands";
import type { Exportacao, Problema, ResumoCapacidades } from "@domain/edicao";
import { separarErro, useEdicaoStore, type ErroComando } from "../store/edicaoStore";
import { useRenderStore } from "../store/renderStore";
import { classificar, contarAvisos, irParaProblema, podeExportar, rotuloExportar, useExportarUi } from "./avisos";
import styles from "./Exportar.module.css";

/** As que a tela oferece; a personalizada não tem campos no §8.6 e só aparece quando o documento já a traz. */
const PREDEFINICOES: { v: Exportacao["predefinicao"]; r: string }[] = [
  { v: "juri", r: "Júri" },
  { v: "whatsapp", r: "WhatsApp" },
  { v: "preservacao", r: "Preservação" },
];
const LEGENDAS: { v: Exportacao["legendas"]; r: string }[] = [
  { v: "nenhuma", r: "Não" },
  { v: "queimar", r: "Na imagem" },
  { v: "arquivo", r: "Arquivo" },
  { v: "ambos", r: "As duas" },
];
const CODEC: Record<NonNullable<Exportacao["personalizada"]>["codec"], string> = { h264: "libx264", hevc: "libx265", av1: "libaom-av1", ffv1: "ffv1" };

/** Codificador que a exportação vai usar: só o júri sem "Arquivo reprodutível" usa a GPU aprovada (§5.7). */
export function codificadorDe(e: Exportacao, cap: ResumoCapacidades): { nome: string; marca: string; gpu: boolean } {
  if (e.predefinicao === "preservacao") return { nome: "ffv1", marca: "ffv1", gpu: false };
  if (e.predefinicao === "personalizada") return { nome: e.personalizada ? CODEC[e.personalizada.codec] : "libx264", marca: "cpu", gpu: false };
  if (e.predefinicao === "whatsapp" || e.codificador === "cpu") return { nome: "libx264", marca: "cpu", gpu: false };
  return { nome: cap.codificador, marca: cap.codificador, gpu: !!cap.gpu };
}
const NOMES: Record<Exportacao["predefinicao"], string> = { juri: "Júri", whatsapp: "WhatsApp", preservacao: "Preservação", personalizada: "Personalizada" };

export function DialogoExportar() {
  const aberto = useExportarUi((s) => s.dialogo);
  const fechar = useExportarUi((s) => s.fecharExportar);
  if (!aberto) return null;
  return <Conteudo fechar={fechar} />;
}

function Conteudo({ fechar }: { fechar: () => void }) {
  const doc = useEdicaoStore((s) => s.doc);
  const so = useEdicaoStore((s) => s.soLeitura);
  const analise = useEdicaoStore((s) => s.analise);
  const atual = useEdicaoStore((s) => s.analiseRevisao === s.revisao && s.analise !== null);
  // análise desta revisão falhou: para de girar e mostra o erro (contratos (h) 27)
  const falhaAnalise = useEdicaoStore((s) => (s.analiseFalhou === s.revisao ? s.erroAnalise : null));
  const [ciencias, setCiencias] = useState<Set<string>>(new Set());
  const [enviando, setEnviando] = useState(false);
  const [erro, setErro] = useState<ErroComando | null>(null);
  const [cap, setCap] = useState<ResumoCapacidades | null>(null);
  const [testando, setTestando] = useState(false);
  const [revisado, setRevisado] = useState<string | null>(null);
  // análise desta revisão que falhou (falha passageira): pedida de novo ao abrir
  useEffect(() => useEdicaoStore.getState().analisarDeNovo(), []);
  useEffect(() => {
    let vivo = true;
    commands
      .edicaoCapacidades(false)
      .then((c) => vivo && setCap(c))
      .catch(() => {});
    return () => {
      vivo = false;
    };
  }, []);
  if (!doc) return null;
  const exp = doc.saida.exportacao;
  const c = classificar(atual && analise ? analise.problemas : []);
  const n = contarAvisos(c);
  const pode = atual && !so && !enviando && podeExportar(c, ciencias);
  const opcoes = PREDEFINICOES.some((p) => p.v === exp.predefinicao) ? PREDEFINICOES : [...PREDEFINICOES, { v: exp.predefinicao, r: NOMES[exp.predefinicao] }];
  const mudar = (rotulo: string, f: (e: Exportacao) => void) => useEdicaoStore.getState().alterar(rotulo, (d) => f(d.saida.exportacao));
  const exportar = async () => {
    setEnviando(true);
    setErro(null);
    try {
      const e = useEdicaoStore.getState().doc?.saida.exportacao ?? exp;
      await useRenderStore.getState().exportar({ tipo: "final", exportacao: e }, [...ciencias].filter((x) => c.periciais.some((g) => g.codigo === x)));
      fechar();
      useExportarUi.getState().mostrarFila(true);
    } catch (x) {
      setErro(separarErro(x));
    } finally {
      setEnviando(false);
    }
  };
  const ir = (p: Problema) => {
    if (irParaProblema(p)) fechar();
  };
  // só o júri escolhe: as outras predefinições codificam sempre em CPU, com threads fixas
  const escolhe = exp.predefinicao === "juri";
  const reprodutivel = exp.codificador === "cpu" || !escolhe;
  const cod = cap ? codificadorDe(exp, cap) : null;
  const testarDeNovo = async () => {
    setTestando(true);
    try {
      setCap(await commands.edicaoCapacidades(true));
    } catch (x) {
      setErro(separarErro(x));
    } finally {
      setTestando(false);
    }
  };
  const gravarRevisado = () => {
    if (revisado !== null && revisado.trim() !== exp.revisado_por) mudar("Revisado por", (x) => void (x.revisado_por = revisado.trim()));
    setRevisado(null);
  };
  return (
    <Dialog
      open
      title="Exportar"
      onClose={fechar}
      footer={
        <>
          <Button variant="ghost" onClick={fechar}>
            Cancelar
          </Button>
          <Button variant="primary" disabled={!pode} onClick={() => void exportar()} leftIcon={enviando || (!atual && !falhaAnalise) ? <Loader2 size={14} className={styles.gira} /> : undefined} data-acao="exportar">
            {rotuloExportar(n)}
          </Button>
        </>
      }
    >
      <div className={styles.campos}>
        <label className={styles.campo}>
          <span className={styles.rotulo}>Predefinição</span>
          <select className={styles.entrada} data-campo="predefinicao" value={exp.predefinicao} disabled={so} onChange={(e) => mudar("Predefinição", (x) => void (x.predefinicao = e.target.value as Exportacao["predefinicao"]))}>
            {opcoes.map((o) => (
              <option key={o.v} value={o.v}>
                {o.r}
              </option>
            ))}
          </select>
        </label>
        <div className={styles.reprodutivel}>
          <label className={styles.check}>
            <input type="checkbox" checked={reprodutivel} disabled={so || !escolhe} onChange={(e) => mudar("Arquivo reprodutível", (x) => void (x.codificador = e.target.checked ? "cpu" : "auto"))} />
            <span>Arquivo reprodutível</span>
          </label>
          {cod && (
            <span className={styles.codificador} data-codificador={cod.marca}>
              {cod.gpu ? <Zap size={13} aria-hidden /> : <Cpu size={13} aria-hidden />}
              <code>{cod.nome}</code>
              <button type="button" className={styles.icone} title="Testar de novo" aria-label="Testar de novo" disabled={testando} onClick={() => void testarDeNovo()}>
                <RefreshCw size={12} className={testando ? styles.gira : undefined} />
              </button>
            </span>
          )}
        </div>
        <label className={styles.campo}>
          <span className={styles.rotulo}>Legendas</span>
          <select className={styles.entrada} data-campo="legendas" value={exp.legendas} disabled={so} onChange={(e) => mudar("Legendas", (x) => void (x.legendas = e.target.value as Exportacao["legendas"]))}>
            {LEGENDAS.map((o) => (
              <option key={o.v} value={o.v}>
                {o.r}
              </option>
            ))}
          </select>
        </label>
        <label className={styles.campo}>
          <span className={styles.rotulo}>Revisado por</span>
          <input
            className={styles.entrada}
            data-campo="revisado_por"
            value={revisado ?? exp.revisado_por}
            disabled={so}
            maxLength={120}
            onChange={(e) => setRevisado(e.target.value)}
            onBlur={gravarRevisado}
            onKeyDown={(e) => e.key === "Enter" && gravarRevisado()}
          />
        </label>
        <div className={styles.pacote}>
          <label className={styles.check}>
            <input type="checkbox" data-campo="pacote" checked={exp.pacote !== null} disabled={so} onChange={(e) => mudar("Pacote do júri", (x) => void (x.pacote = e.target.checked ? { originais: false } : null))} />
            <span>Pacote do júri</span>
          </label>
          <label className={styles.check}>
            <input
              type="checkbox"
              data-campo="originais"
              checked={exp.pacote?.originais ?? false}
              disabled={so || exp.pacote === null}
              onChange={(e) => mudar("Incluir originais", (x) => void (x.pacote = { originais: e.target.checked }))}
            />
            <span>Incluir originais</span>
          </label>
          <label className={styles.check}>
            <input
              type="checkbox"
              data-campo="pdf"
              checked={exp.pacote !== null && exp.pdf === "a2_sem_anexos"}
              disabled={so || exp.pacote === null}
              onChange={(e) => mudar("PDF sem anexos", (x) => void (x.pdf = e.target.checked ? "a2_sem_anexos" : "a3"))}
            />
            <span>PDF sem anexos</span>
          </label>
        </div>
      </div>
      {(c.erros.length > 0 || n > 0) && (
        <ul className={styles.problemas}>
          {c.erros.map((p, i) => (
            <li key={`e${i}`} className={styles.erro} data-nivel="erro">
              <button type="button" className={styles.salto} onClick={() => ir(p)}>
                <OctagonX size={14} aria-hidden />
                <span data-dado>{p.mensagem}</span>
                <ChevronRight size={13} className={styles.seta} aria-hidden />
              </button>
            </li>
          ))}
          {c.periciais.map((g) => (
            <li key={`p${g.codigo}`} className={styles.pericial} data-nivel="pericial" data-codigo={g.codigo}>
              <button type="button" className={styles.salto} onClick={() => ir(g.itens[0] as Problema)}>
                <ShieldAlert size={14} aria-hidden />
                <span data-dado>{g.mensagem}</span>
                {g.itens.length > 1 && <span className={styles.conta} data-dado>{g.itens.length}</span>}
              </button>
              <label className={styles.ciente}>
                <input
                  type="checkbox"
                  checked={ciencias.has(g.codigo)}
                  onChange={(e) =>
                    setCiencias((s) => {
                      const novo = new Set(s);
                      if (e.target.checked) novo.add(g.codigo);
                      else novo.delete(g.codigo);
                      return novo;
                    })
                  }
                />
                <span>Ciente</span>
              </label>
            </li>
          ))}
          {c.avisos.map((p, i) => (
            <li key={`a${i}`} className={styles.aviso} data-nivel="aviso">
              <button type="button" className={styles.salto} onClick={() => ir(p)}>
                <AlertTriangle size={14} aria-hidden />
                <span data-dado>{p.mensagem}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
      {erro ? (
        <p className={styles.erroLinha} data-dado>{erro.frase || erro.codigo}</p>
      ) : (
        falhaAnalise && <p className={styles.erroLinha} data-dado data-erro-analise="">{falhaAnalise.frase || falhaAnalise.codigo}</p>
      )}
    </Dialog>
  );
}
