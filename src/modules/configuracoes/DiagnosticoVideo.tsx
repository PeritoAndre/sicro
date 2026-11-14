/**
 * Configurações → Vídeo (P0.9, arquitetura §5.6, §7.7, O4): capacidades do ffmpeg e da GPU, e o Diagnóstico de
 * vídeo, que roda as medições da prévia no webview do app e grava `diagnostico-<data>.json`.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  Circle,
  Copy,
  Cpu,
  FolderOpen,
  Gauge,
  Loader2,
  Play,
  RefreshCw,
  Square,
  XCircle,
} from "lucide-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { CodificadorReprovado, ResumoCapacidades } from "@domain/edicao";
import { edicaoDiagnosticoMidias, edicaoDiagnosticoSalvar, type MidiasDiagnostico } from "@modules/edicao/diagnostico/comandos";
import {
  diagnosticar,
  MEDIDAS,
  type Estado,
  type Linha,
  type Progresso,
  type Resultado,
} from "@modules/edicao/diagnostico/executar";
import { ambienteDoNavegador, navegadorAtual, type AmbienteDoNavegador } from "@modules/edicao/diagnostico/navegador";
import { resumir } from "@modules/edicao/diagnostico/resumo";
import styles from "./DiagnosticoVideo.module.css";

const ORIGEM: Record<ResumoCapacidades["origem"], string> = {
  configurado: "Configurado",
  embutido: "Embutido",
  path: "Sistema",
};

const CLASSE: Record<CodificadorReprovado["classe"], string> = {
  sem_gpu: "Sem GPU",
  sem_driver: "Sem driver",
  driver_antigo: "Driver antigo",
  limite_de_sessoes: "Limite de sessões",
  recurso_nao_suportado: "Recurso não suportado",
  ffmpeg_sem_codificador: "ffmpeg sem codificador",
  saida_invalida: "Saída inválida",
  outra: "Outra",
};

function IconeEstado({ e }: { e: Estado }) {
  if (e === "rodando") return <Loader2 size={14} className={styles.girando} aria-label="rodando" />;
  if (e === "feita") return <CheckCircle2 size={14} className={styles.ok} aria-label="feita" />;
  if (e === "falhou") return <XCircle size={14} className={styles.falha} aria-label="falhou" />;
  return <Circle size={14} className={styles.espera} aria-label="espera" />;
}

function Capacidades() {
  const [cap, setCap] = useState<ResumoCapacidades | null>(null);
  const [erro, setErro] = useState<string | null>(null);
  const [testando, setTestando] = useState(false);

  const ler = useCallback(async (deNovo: boolean) => {
    setErro(null);
    try {
      setCap(await commands.edicaoCapacidades(deNovo));
    } catch (e) {
      setErro(toSicroError(e).message);
    }
  }, []);

  useEffect(() => {
    void ler(false);
  }, [ler]);

  // sonda de GPU em segundo plano: relê até acabar
  useEffect(() => {
    if (!cap?.sondando) return;
    const t = window.setTimeout(() => void ler(false), 2000);
    return () => window.clearTimeout(t);
  }, [cap, ler]);

  const testar = async () => {
    setTestando(true);
    await ler(true);
    setTestando(false);
  };

  const faltam = cap?.faltam ?? [];
  const opcionais = cap?.opcionais_ausentes ?? [];

  return (
    <section className={styles.card}>
      <div className={styles.cardHead}>
        <span className={styles.head}>
          <Cpu size={15} aria-hidden />
          <h2 className={styles.title}>Capacidades</h2>
        </span>
        <Button
          variant="ghost"
          size="sm"
          leftIcon={<RefreshCw size={13} className={testando || cap?.sondando ? styles.girando : undefined} />}
          onClick={() => void testar()}
          disabled={testando}
        >
          Testar de novo
        </Button>
      </div>
      {erro && <div className={styles.erroLinha}>{erro}</div>}
      {cap && (
        <dl className={styles.tabela}>
          <dt>ffmpeg</dt>
          <dd>
            <span className={styles.valor}>{cap.versao}</span>
            <span className={styles.dim}>{ORIGEM[cap.origem]}</span>
            <span className={styles.mono} title={cap.sha256}>
              {cap.sha256.slice(0, 12)}
            </span>
          </dd>

          <dt>Filtros</dt>
          <dd>
            {faltam.length ? (
              <XCircle size={14} className={styles.falha} aria-label="faltam" />
            ) : (
              <CheckCircle2 size={14} className={styles.ok} aria-label="completo" />
            )}
            {faltam.length > 0 && <span className={styles.lista}>{faltam.join(" · ")}</span>}
            {opcionais.length > 0 && (
              <>
                <AlertTriangle size={14} className={styles.aviso} aria-label="opcionais ausentes" />
                <span className={`${styles.lista} ${styles.dim}`}>{opcionais.join(" · ")}</span>
              </>
            )}
          </dd>

          <dt>Codificador</dt>
          <dd className={styles.coluna}>
            <span className={styles.linha}>
              {cap.sondando ? (
                <Loader2 size={14} className={styles.girando} aria-label="sondando" />
              ) : cap.gpu ? (
                <CheckCircle2 size={14} className={styles.ok} aria-label="GPU" />
              ) : (
                <Cpu size={14} className={styles.dim} aria-label="CPU" />
              )}
              <span className={styles.valor}>{cap.codificador}</span>
              {cap.gpu && (
                <>
                  <span className={styles.mono}>{cap.gpu.degrau}</span>
                  <span className={styles.dim}>{cap.gpu.nome}</span>
                  <span className={styles.dim}>{cap.gpu.driver}</span>
                </>
              )}
            </span>
            {cap.reprovados.map((r) => (
              <span key={r.codificador} className={styles.reprovado}>
                <XCircle size={13} className={styles.falha} aria-label="reprovado" />
                <span className={styles.mono}>{r.codificador}</span>
                <span className={styles.motivo}>{CLASSE[r.classe]}</span>
                {r.driver_minimo && <span className={styles.dim}>Driver mínimo {r.driver_minimo}</span>}
                {r.api_exigida && <span className={styles.dim}>API exigida {r.api_exigida}</span>}
                {r.api_do_driver && <span className={styles.dim}>API do driver {r.api_do_driver}</span>}
                {r.dica && <span className={styles.dica}>{r.dica}</span>}
              </span>
            ))}
          </dd>
        </dl>
      )}
    </section>
  );
}

type Fase = "parado" | "rodando" | "pronto" | "falhou";

const ESTADOS_INICIAIS: Record<Linha, Estado> = { midias: "espera", tempo3: "espera", pool8: "espera", compose: "espera", switch: "espera" };

function Diagnostico() {
  const [fase, setFase] = useState<Fase>("parado");
  const [prog, setProg] = useState<Progresso | null>(null);
  const [res, setRes] = useState<(Resultado & { midias: MidiasDiagnostico }) | null>(null);
  const [erro, setErro] = useState<string | null>(null);
  const [copiado, setCopiado] = useState(false);
  const palco = useRef<HTMLCanvasElement>(null);
  const caixa = useRef<HTMLDivElement>(null);
  const abortar = useRef<AbortController | null>(null);

  useEffect(() => () => abortar.current?.abort(), []);

  // os <video> ficam no palco: fora da área visível o WebKit pode pausá-los
  useEffect(() => {
    if (fase === "rodando") caixa.current?.scrollIntoView?.({ block: "nearest" });
  }, [fase]);

  const medir = async () => {
    // duplo clique antes de o botão virar "Cancelar": uma medição por vez
    if (!palco.current || !caixa.current || abortar.current) return;
    const ctrl = new AbortController();
    abortar.current = ctrl;
    setFase("rodando");
    setErro(null);
    setRes(null);
    setProg(null);
    const vivo: { amb: AmbienteDoNavegador | null; midias: MidiasDiagnostico | null } = { amb: null, midias: null };
    const p = palco.current;
    const c = caixa.current;
    try {
      const r = await diagnosticar({
        midias: async (av) => (vivo.midias = await edicaoDiagnosticoMidias(av)),
        ambiente: (m) => (vivo.amb = ambienteDoNavegador({ midias: m, palco: p, caixa: c, sinal: ctrl.signal })),
        salvar: (j) => edicaoDiagnosticoSalvar(j),
        navegador: navegadorAtual,
        agora: () => new Date(),
        aoProgresso: (pr) => {
          if (!ctrl.signal.aborted) setProg(pr);
        },
      });
      if (ctrl.signal.aborted) return;
      if (r && vivo.midias) {
        setRes({ ...r, midias: vivo.midias });
        if (r.erro) setErro(r.erro);
        setFase("pronto");
      } else setFase("parado");
    } catch (e) {
      if (ctrl.signal.aborted) return;
      setErro(toSicroError(e).message);
      setFase("falhou");
    } finally {
      vivo.amb?.limpar();
      if (abortar.current === ctrl) abortar.current = null;
    }
  };

  const cancelar = () => {
    abortar.current?.abort();
    abortar.current = null;
    setProg(null);
    setFase("parado");
  };

  const copiar = async () => {
    if (!res) return;
    const txt = JSON.stringify(res.relatorio, null, 2);
    try {
      await writeText(txt);
    } catch {
      try {
        await navigator.clipboard.writeText(txt);
      } catch {
        return;
      }
    }
    setCopiado(true);
    window.setTimeout(() => setCopiado(false), 1500);
  };

  const estados = prog?.estados ?? ESTADOS_INICIAIS;
  const linhas = res ? resumir(res.relatorio, (n) => res.midias.midias.find((m) => m.nome === n)?.p0_s) : [];

  return (
    <section className={styles.card}>
      <div className={styles.cardHead}>
        <span className={styles.head}>
          <Gauge size={15} aria-hidden />
          <h2 className={styles.title}>Diagnóstico</h2>
        </span>
        <span className={styles.acoes}>
          {fase === "rodando" && prog && (
            <span className={styles.contagem}>
              {prog.feitas}/{prog.total}
            </span>
          )}
          {fase === "rodando" ? (
            <Button variant="secondary" size="sm" leftIcon={<Square size={13} />} onClick={cancelar}>
              Cancelar
            </Button>
          ) : (
            <Button variant="primary" size="sm" leftIcon={<Play size={13} />} onClick={() => void medir()}>
              Medir
            </Button>
          )}
        </span>
      </div>

      <div className={styles.corpo}>
        <ul className={styles.etapas}>
          <li>
            <IconeEstado e={estados.midias} />
            <span className={styles.rotulo}>Mídias</span>
            {fase === "rodando" && estados.midias === "rodando" && prog?.midias && (
              <span className={styles.contagem}>
                {prog.midias[0]}/{prog.midias[1]}
              </span>
            )}
          </li>
          {MEDIDAS.map((m) => (
            <li key={m}>
              <IconeEstado e={estados[m]} />
              <span className={`${styles.rotulo} ${styles.mono}`}>{m}</span>
            </li>
          ))}
        </ul>
        {/* palco: canvas das medições e os <video> de 2×2 px; só aparece depois do primeiro clique */}
        <div ref={caixa} className={styles.palco} data-vazio={fase === "parado" && !res ? "" : undefined}>
          <canvas ref={palco} className={styles.canvas} width={1280} height={720} />
        </div>
      </div>

      {erro && <div className={styles.erroLinha}>{erro}</div>}

      {res && (
        <>
          <table className={styles.resultado}>
            <tbody>
              {linhas.map((l) => (
                <tr key={`${l.medida}/${l.variante}`}>
                  <td className={styles.mono}>{l.medida}</td>
                  <td>{l.variante}</td>
                  <td className={styles.valores}>
                    {l.erro ? (
                      <span className={styles.falhaTexto} title={l.erro}>
                        <XCircle size={13} aria-label="falhou" /> {l.erro}
                      </span>
                    ) : (
                      l.valores.map((v) => (
                        <span key={v.rotulo} className={styles.par}>
                          <span className={styles.dim}>{v.rotulo}</span>
                          <span className={v.ok === false ? styles.falhaTexto : v.ok ? styles.okTexto : styles.valor}>{v.valor}</span>
                        </span>
                      ))
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className={styles.rodape}>
            <span className={styles.caminho} title={res.caminho ?? undefined}>
              {res.caminho}
            </span>
            <Button variant="ghost" size="sm" leftIcon={copiado ? <Check size={13} /> : <Copy size={13} />} onClick={() => void copiar()}>
              Copiar
            </Button>
            {res.caminho && (
              <Button
                variant="ghost"
                size="sm"
                leftIcon={<FolderOpen size={13} />}
                onClick={() => res.caminho && void commands.revealPathInExplorer(res.caminho).catch(() => {})}
              >
                Abrir pasta
              </Button>
            )}
          </div>
        </>
      )}
    </section>
  );
}

export function DiagnosticoVideo() {
  return (
    <div className={styles.pilha}>
      <Capacidades />
      <Diagnostico />
    </div>
  );
}

