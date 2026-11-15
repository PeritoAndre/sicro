/** Gerenciador de IA: baixa whisper.cpp + modelos sob demanda (opt-in) e auto-configura os caminhos. */

import { useCallback, useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  Circle,
  Cpu,
  Download,
  Loader2,
  RefreshCw,
  ScanSearch,
  Sparkles,
  Trash2,
  Users,
  XCircle,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useSettingsStore } from "@stores/settingsStore";
import type {
  AiCatalog,
  AiProgress,
  AiStatus,
  AiUpdateInfo,
  CatalogItem,
  EstadoIa,
} from "@domain/ai";
import styles from "./AiManagerCard.module.css";

function prettyMB(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(0)} MB`;
}

export function AiManagerCard() {
  const [catalog, setCatalog] = useState<AiCatalog | null>(null);
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [progress, setProgress] = useState<Record<string, AiProgress>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [update, setUpdate] = useState<AiUpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);

  const reloadStatus = useCallback(async () => {
    try {
      setStatus(await commands.getAiStatus());
    } catch {
      /* best-effort */
    }
  }, []);

  useEffect(() => {
    void commands
      .getAiCatalog()
      .then(setCatalog)
      .catch((e) => setError(toSicroError(e).message));
    void reloadStatus();
  }, [reloadStatus]);

  useEffect(() => {
    let un: (() => void) | undefined;
    void listen<AiProgress>("ai-download-progress", (e) => {
      setProgress((prev) => ({ ...prev, [e.payload.id]: e.payload }));
    }).then((u) => {
      un = u;
    });
    return () => un?.();
  }, []);

  const install = async (item: CatalogItem) => {
    setBusy(item.id);
    setError(null);
    setProgress((p) => ({ ...p, [item.id]: { id: item.id, received: 0, total: 0 } }));
    try {
      const st = await commands.installAiAsset(item.id);
      setStatus(st);
      // A tela de degravação passa a enxergar os caminhos auto-configurados.
      await useSettingsStore.getState().load();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
      setProgress((p) => {
        const n = { ...p };
        delete n[item.id];
        return n;
      });
    }
  };

  const remove = async (item: CatalogItem) => {
    setBusy(item.id);
    setError(null);
    try {
      setStatus(await commands.removeAiAsset(item.id));
      await useSettingsStore.getState().load();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
    }
  };

  const checkUpdates = async () => {
    setChecking(true);
    setError(null);
    try {
      setUpdate(await commands.checkAiUpdates());
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setChecking(false);
    }
  };

  // Opt-in; a versão usada fica registrada no log da degravação.
  const updateEngine = async () => {
    const installedBuild = catalog?.items.find(
      (i) => i.kind === "build" && !!status?.whisper_bin_path.includes(i.id),
    );
    const pid = installedBuild?.id ?? "whisper-update";
    setBusy(pid);
    setError(null);
    setProgress((p) => ({ ...p, [pid]: { id: pid, received: 0, total: 0 } }));
    try {
      setStatus(await commands.updateWhisperEngine());
      await useSettingsStore.getState().load();
      setUpdate(await commands.checkAiUpdates());
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
      setProgress((p) => {
        const n = { ...p };
        delete n[pid];
        return n;
      });
    }
  };

  const installDiar = async () => {
    setBusy(DIAR_ID);
    setError(null);
    setProgress((p) => ({ ...p, [DIAR_ID]: { id: DIAR_ID, received: 0, total: 0 } }));
    try {
      setStatus(await commands.installDiarization());
      await useSettingsStore.getState().load();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
      setProgress((p) => {
        const n = { ...p };
        delete n[DIAR_ID];
        return n;
      });
    }
  };

  const removeDiar = async () => {
    setBusy(DIAR_ID);
    setError(null);
    try {
      setStatus(await commands.removeDiarization());
      await useSettingsStore.getState().load();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
    }
  };

  const installPessoas = async () => {
    setBusy(PESSOAS_ID);
    setError(null);
    setProgress((p) => ({ ...p, [PESSOAS_ID]: { id: PESSOAS_ID, received: 0, total: 0 } }));
    try {
      setStatus(await commands.installPessoas());
      await useSettingsStore.getState().load();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
      setProgress((p) => {
        const n = { ...p };
        delete n[PESSOAS_ID];
        return n;
      });
    }
  };

  const removePessoas = async () => {
    setBusy(PESSOAS_ID);
    setError(null);
    try {
      setStatus(await commands.removePessoas());
      await useSettingsStore.getState().load();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
    }
  };

  const installSuperres = async () => {
    setBusy(SUPERRES_ID);
    setError(null);
    setProgress((p) => ({ ...p, [SUPERRES_ID]: { id: SUPERRES_ID, received: 0, total: 0 } }));
    try {
      setStatus(await commands.installSuperres());
      await useSettingsStore.getState().load();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
      setProgress((p) => {
        const n = { ...p };
        delete n[SUPERRES_ID];
        return n;
      });
    }
  };

  const removeSuperres = async () => {
    setBusy(SUPERRES_ID);
    setError(null);
    try {
      setStatus(await commands.removeSuperres());
      await useSettingsStore.getState().load();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
    }
  };

  const isInstalled = (item: CatalogItem): boolean => {
    if (!status) return false;
    if (item.kind === "build") {
      return status.whisper_ok && status.whisper_bin_path.includes(item.id);
    }
    return status.installed_models.some((m) => m.filename === item.filename);
  };

  const builds = catalog?.items.filter((i) => i.kind === "build") ?? [];
  // "Modelos" inclui os de transcrição e o VAD (anti-alucinação).
  const models = catalog?.items.filter((i) => i.kind !== "build") ?? [];

  return (
    <section className={styles.card}>
      <div className={styles.head}>
        <Bot size={18} aria-hidden />
        <h2 className={styles.title}>Degravação por IA (local)</h2>
      </div>
      <p className={styles.desc}>
        Opcional e 100% offline. Baixe o motor whisper.cpp e um modelo uma vez — o
        SICRO configura tudo sozinho. A degravação é sempre um{" "}
        <strong>rascunho</strong> que você revisa; nada é instalado ou trocado
        automaticamente.
      </p>

      <div className={styles.statusRow}>
        <span className={status?.whisper_ok ? styles.ok : styles.off}>
          {status?.whisper_ok && <CheckCircle2 size={13} aria-hidden />} Motor:{" "}
          {status?.whisper_ok
            ? `instalado (${status.whisper_version || "?"})`
            : "não instalado"}
        </span>
        <span className={status?.model_ok ? styles.ok : styles.off}>
          {status?.model_ok && <CheckCircle2 size={13} aria-hidden />} Modelo:{" "}
          {status?.model_ok
            ? (status.model_path.split(/[\\/]/).pop() ?? "ok")
            : "nenhum"}
        </span>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      <IaGeral progress={progress} />

      <h3 className={styles.group}>Motor (whisper.cpp)</h3>
      {builds.map((item) => (
        <AssetRow
          key={item.id}
          item={item}
          catalog={catalog}
          installed={isInstalled(item)}
          busy={busy}
          progress={progress[item.id]}
          updateAvailable={!!update?.update_available}
          updateLatest={update?.latest}
          onInstall={() => void install(item)}
          onRemove={() => void remove(item)}
          onUpdate={() => void updateEngine()}
        />
      ))}

      <h3 className={styles.group}>Modelos</h3>
      {models.map((item) => (
        <AssetRow
          key={item.id}
          item={item}
          catalog={catalog}
          installed={isInstalled(item)}
          busy={busy}
          progress={progress[item.id]}
          onInstall={() => void install(item)}
          onRemove={() => void remove(item)}
        />
      ))}

      {status?.diar_available && (
        <>
          <h3 className={styles.group}>
            <Users size={13} aria-hidden /> Separação de locutores
          </h3>
          <DiarRow
            ok={status.diar_ok}
            version={status.diar_version}
            note={
              <>
                ≈ {status.diar_approx_mb} MB · Diz <strong>quem fala quando</strong> na degravação
                (segmentação pyannote 3.0 + assinatura de voz WeSpeaker). Arquivos oficiais do
                k2-fsa/sherpa-onnx com hash conferido; depois de baixado, roda offline. Não
                identifica pessoas — separa vozes diferentes.
              </>
            }
            busy={busy}
            progress={progress[DIAR_ID]}
            onInstall={() => void installDiar()}
            onRemove={() => void removeDiar()}
          />
        </>
      )}

      {status?.pessoas_available && (
        <>
          <h3 className={styles.group}>
            <ScanSearch size={13} aria-hidden /> Rastreio no vídeo
          </h3>
          <DiarRow
            id={PESSOAS_ID}
            label="Detecção de objetos — YOLOX-S + onnxruntime"
            ok={status.pessoas_ok}
            version={status.pessoas_version}
            note={
              <>
                ≈ {status.pessoas_approx_mb} MB · Numera pessoas, veículos e outros objetos da cena e
                acompanha cada um no vídeo (aba Rastreio). Modelo YOLOX (Apache-2.0, Megvii) e
                onnxruntime (MIT, Microsoft), com hash conferido; depois de baixado, roda
                offline. Só localiza — não identifica ninguém.
              </>
            }
            busy={busy}
            progress={progress[PESSOAS_ID]}
            onInstall={() => void installPessoas()}
            onRemove={() => void removePessoas()}
          />
        </>
      )}

      {status?.superres_available && (
        <>
          <h3 className={styles.group}>
            <Sparkles size={13} aria-hidden /> Super-resolução
          </h3>
          <DiarRow
            id={SUPERRES_ID}
            label="Super-resolução — Real-ESRGAN + onnxruntime"
            ok={status.superres_ok}
            version={status.superres_version}
            note={
              <>
                ≈ {status.superres_approx_mb} MB · Amplia 2x ou 4x um quadro ou uma região (aba Realce
                do vídeo e filtros da imagem). Modelo Real-ESRGAN general-x4v3 (BSD-3) e onnxruntime
                (MIT), com hash conferido; roda na CPU, offline. A rede pode inventar detalhe: o
                resultado é só leitura, nunca prova.
              </>
            }
            busy={busy}
            progress={progress[SUPERRES_ID]}
            onInstall={() => void installSuperres()}
            onRemove={() => void removeSuperres()}
          />
        </>
      )}

      <div className={styles.footer}>
        <Button
          variant="ghost"
          size="sm"
          leftIcon={<RefreshCw size={13} />}
          onClick={() => void checkUpdates()}
          disabled={checking}
        >
          {checking ? "Verificando…" : "Verificar atualizações"}
        </Button>
        {update && (
          <span className={styles.updateInfo}>
            {update.update_available
              ? `Nova versão do whisper.cpp disponível: ${update.latest} (você tem ${update.current}).`
              : `Você está na versão mais recente (${update.current}).`}
          </span>
        )}
      </div>
    </section>
  );
}

/** Frases do catálogo (`edicao/mensagens.rs`) dos códigos que o Gerenciador mostra; conferidas por teste. */
export const FRASES_IA: Readonly<Record<string, string>> = {
  ia_desligada: "Modelos de IA desligados",
  reabrir: "Reabrir o SICRO",
  gpu_nao_instalada: "Aceleração por GPU não instalada",
  gpu_sem_bibliotecas: "Faltam bibliotecas da NVIDIA (CUDA)",
  gpu_sem_cudnn: "Falta o cuDNN ou ele está incompleto",
  gpu_sem_placa: "Nenhuma placa NVIDIA disponível",
  gpu_placa_antiga: "Placa NVIDIA sem suporte",
  gpu_driver_antigo: "Driver da NVIDIA antigo",
  gpu_sem_memoria: "Memória de vídeo insuficiente",
  gpu_falhou: "A GPU não pôde ser usada",
};

/** Erro de comando ("validation error: <código>: <frase>") → a frase. */
function fraseDoErro(e: unknown): string {
  const m = toSicroError(e).message.replace(/^[a-z/]+ error: /, "");
  const cod = /^([a-z][a-z0-9_]*)(?::|$)/.exec(m)?.[1];
  if (cod && FRASES_IA[cod]) return FRASES_IA[cod];
  return m.replace(/^[a-z][a-z0-9_]*: /, "");
}

const GPU_ID = "gpu";
const RASTREADOR_ID = "rastreador";

/** Interruptor "Usar modelos de IA" e os itens da Fase 2: aceleração por GPU e rastreador (§6.5, §6.6). */
export function IaGeral({ progress }: { progress: Record<string, AiProgress> }) {
  const settings = useSettingsStore((s) => s.settings);
  const [estado, setEstado] = useState<EstadoIa | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [erro, setErro] = useState<string | null>(null);

  const ler = useCallback(async () => {
    try {
      setEstado(await commands.iaEstado());
    } catch (e) {
      setErro(fraseDoErro(e));
    }
  }, []);

  useEffect(() => {
    void ler();
  }, [ler]);

  const agir = async (id: string, f: () => Promise<EstadoIa>) => {
    setBusy(id);
    setErro(null);
    try {
      setEstado(await f());
      await useSettingsStore.getState().load();
    } catch (e) {
      setErro(fraseDoErro(e));
    } finally {
      setBusy(null);
    }
  };

  const ligar = async (ligada: boolean) => {
    setErro(null);
    try {
      const s = useSettingsStore.getState().settings;
      await useSettingsStore.getState().persist({ ...s, ai: { ...s.ai, modelos_desligados: !ligada } });
      await ler();
    } catch (e) {
      setErro(fraseDoErro(e));
    }
  };

  const gpu = estado?.gpu;
  const motivoGpu = gpu ? (gpu.requisito ?? (gpu.instalada && !gpu.pronta ? gpu.motivo : null)) : null;

  return (
    <>
      <label className={styles.interruptor}>
        <input
          type="checkbox"
          checked={!settings.ai.modelos_desligados}
          onChange={(e) => void ligar(e.target.checked)}
        />
        Usar modelos de IA
      </label>
      {erro && <div className={styles.error}>{erro}</div>}

      {gpu?.disponivel && (
        <ItemIa
          id={GPU_ID}
          rotulo="Aceleração por GPU (NVIDIA)"
          estado={busy === GPU_ID ? "ocupado" : gpu.pronta ? "pronto" : gpu.instalada ? "aviso" : gpu.requisito ? "falha" : "ausente"}
          mb={gpu.mb}
          nota="bibliotecas da NVIDIA, sob licença própria"
          motivo={motivoGpu ? (FRASES_IA[motivoGpu] ?? motivoGpu) : null}
          reabrir={estado?.reabrir ?? false}
          instalado={gpu.instalada}
          podeBaixar={!gpu.requisito}
          busy={busy}
          progress={progress[GPU_ID]}
          onBaixar={() => void agir(GPU_ID, () => commands.iaGpuInstalar())}
          onRemover={() => void agir(GPU_ID, () => commands.iaGpuRemover())}
          onTestar={() => void agir(GPU_ID, () => commands.iaGpuAutoteste())}
        />
      )}
      {estado && (
        <ItemIa
          id={RASTREADOR_ID}
          rotulo="Rastreador"
          estado={busy === RASTREADOR_ID ? "ocupado" : estado.rastreador ? "pronto" : "ausente"}
          mb={estado.rastreador_mb}
          motivo={null}
          reabrir={false}
          instalado={estado.rastreador}
          podeBaixar
          busy={busy}
          progress={progress[RASTREADOR_ID]}
          onBaixar={() => void agir(RASTREADOR_ID, () => commands.iaRastreadorInstalar())}
          onRemover={() => void agir(RASTREADOR_ID, () => commands.iaRastreadorRemover())}
        />
      )}
    </>
  );
}

type EstadoItem = "pronto" | "aviso" | "falha" | "ausente" | "ocupado";

function IconeItem({ e }: { e: EstadoItem }) {
  if (e === "ocupado") return <Loader2 size={14} className={styles.girando} aria-label="ocupado" />;
  if (e === "pronto") return <CheckCircle2 size={14} className={styles.okIcone} aria-label="pronto" />;
  if (e === "aviso") return <AlertTriangle size={14} className={styles.avisoIcone} aria-label="não pronto" />;
  if (e === "falha") return <XCircle size={14} className={styles.falhaIcone} aria-label="sem suporte" />;
  return <Circle size={14} className={styles.offIcone} aria-label="não instalado" />;
}

function ItemIa(p: {
  id: string;
  rotulo: string;
  estado: EstadoItem;
  mb: number;
  nota?: string;
  motivo: string | null;
  reabrir: boolean;
  instalado: boolean;
  podeBaixar: boolean;
  busy: string | null;
  progress?: AiProgress;
  onBaixar: () => void;
  onRemover: () => void;
  onTestar?: () => void;
}) {
  const ocupado = p.busy === p.id;
  const pct = p.progress && p.progress.total > 0 ? Math.round((p.progress.received / p.progress.total) * 100) : null;
  return (
    <div className={styles.row} data-item={p.id}>
      <div className={styles.rowMain}>
        <div className={styles.rowLabel}>
          <IconeItem e={p.estado} />
          {p.rotulo}
          <span className={styles.rowNote}>{p.mb} MB</span>
        </div>
        {p.nota && <div className={styles.rowNote}>{p.nota}</div>}
        {p.motivo && <div className={styles.erroLinha}>{p.motivo}</div>}
        {p.reabrir && <div className={styles.erroLinha}>{FRASES_IA.reabrir}</div>}
        {ocupado && !p.instalado && (
          <div className={styles.progress}>
            <div className={styles.progressFill} style={{ width: `${pct ?? 0}%` }} />
            <span className={styles.progressTxt}>{pct != null ? `${pct}%` : ""}</span>
          </div>
        )}
      </div>
      <div className={styles.rowActions}>
        {p.instalado ? (
          <>
            {p.onTestar && (
              <Button variant="ghost" size="sm" leftIcon={<Cpu size={13} />} onClick={p.onTestar} disabled={p.busy !== null}>
                Testar de novo
              </Button>
            )}
            <Button variant="ghost" size="sm" leftIcon={<Trash2 size={13} />} onClick={p.onRemover} disabled={p.busy !== null}>
              Remover
            </Button>
          </>
        ) : (
          <Button
            variant="secondary"
            size="sm"
            leftIcon={<Download size={13} />}
            onClick={p.onBaixar}
            disabled={p.busy !== null || !p.podeBaixar}
          >
            {ocupado ? "Baixando…" : "Baixar"}
          </Button>
        )}
      </div>
    </div>
  );
}

const DIAR_ID = "diarizacao";
const PESSOAS_ID = "pessoas";
const SUPERRES_ID = "superres";

/** Pacotes de vários arquivos (separador de locutores, pessoas no vídeo): uma barra só. */
function DiarRow({
  id = DIAR_ID,
  label = "Separador de locutores — sherpa-onnx",
  ok,
  version,
  note,
  busy,
  progress,
  onInstall,
  onRemove,
}: {
  id?: string;
  label?: string;
  ok: boolean;
  version: string;
  note: ReactNode;
  busy: string | null;
  progress?: AiProgress;
  onInstall: () => void;
  onRemove: () => void;
}) {
  const isBusy = busy === id;
  const pct =
    progress && progress.total > 0 ? Math.round((progress.received / progress.total) * 100) : null;
  return (
    <div className={styles.row}>
      <div className={styles.rowMain}>
        <div className={styles.rowLabel}>
          {label}
          {ok && <span className={styles.tagOk}>instalado ({version || "?"})</span>}
        </div>
        <div className={styles.rowNote}>{note}</div>
        {isBusy && (
          <div className={styles.progress}>
            <div className={styles.progressFill} style={{ width: `${pct ?? 0}%` }} />
            <span className={styles.progressTxt}>
              {pct != null ? `${pct}%` : "baixando…"}
              {progress && progress.total > 0
                ? ` (${prettyMB(progress.received)} / ${prettyMB(progress.total)})`
                : ""}
            </span>
          </div>
        )}
      </div>
      <div className={styles.rowActions}>
        {ok ? (
          <Button
            variant="ghost"
            size="sm"
            leftIcon={<Trash2 size={13} />}
            onClick={onRemove}
            disabled={busy !== null}
          >
            Remover
          </Button>
        ) : (
          <Button
            variant="secondary"
            size="sm"
            leftIcon={<Download size={13} />}
            onClick={onInstall}
            disabled={busy !== null}
          >
            {isBusy ? "Baixando…" : "Baixar"}
          </Button>
        )}
      </div>
    </div>
  );
}

function AssetRow({
  item,
  catalog,
  installed,
  busy,
  progress,
  updateAvailable,
  updateLatest,
  onInstall,
  onRemove,
  onUpdate,
}: {
  item: CatalogItem;
  catalog: AiCatalog | null;
  installed: boolean;
  busy: string | null;
  progress?: AiProgress;
  updateAvailable?: boolean;
  updateLatest?: string;
  onInstall: () => void;
  onRemove: () => void;
  onUpdate?: () => void;
}) {
  const isBusy = busy === item.id;
  const recommended = item.kind === "build" && item.gpu && !!catalog?.gpu_detected;
  const pct =
    progress && progress.total > 0
      ? Math.round((progress.received / progress.total) * 100)
      : null;

  return (
    <div className={styles.row}>
      <div className={styles.rowMain}>
        <div className={styles.rowLabel}>
          {item.label}
          {recommended && <span className={styles.tag}>recomendado p/ sua GPU</span>}
          {installed && <span className={styles.tagOk}>instalado</span>}
        </div>
        <div className={styles.rowNote}>
          ≈ {item.approx_mb} MB · {item.note}
        </div>
        {isBusy && (
          <div className={styles.progress}>
            <div className={styles.progressFill} style={{ width: `${pct ?? 0}%` }} />
            <span className={styles.progressTxt}>
              {pct != null ? `${pct}%` : "baixando…"}
              {progress && progress.total > 0
                ? ` (${prettyMB(progress.received)} / ${prettyMB(progress.total)})`
                : ""}
            </span>
          </div>
        )}
      </div>
      <div className={styles.rowActions}>
        {installed ? (
          <>
            {item.kind === "build" && updateAvailable && onUpdate && (
              <Button
                variant="primary"
                size="sm"
                leftIcon={<Download size={13} />}
                onClick={onUpdate}
                disabled={busy !== null}
                title={updateLatest ? `Atualizar para ${updateLatest}` : "Atualizar motor"}
              >
                {isBusy ? "Atualizando…" : "Atualizar"}
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              leftIcon={<Trash2 size={13} />}
              onClick={onRemove}
              disabled={busy !== null}
            >
              Remover
            </Button>
          </>
        ) : (
          <Button
            variant="secondary"
            size="sm"
            leftIcon={<Download size={13} />}
            onClick={onInstall}
            disabled={busy !== null}
          >
            {isBusy ? "Baixando…" : "Baixar"}
          </Button>
        )}
      </div>
    </div>
  );
}
