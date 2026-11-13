/** Coluna direita da tela Início: ações, versão, backup e atalhos. */

import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Download, FolderOpen, Plus, RefreshCw } from "lucide-react";
import { Button } from "@components/Button/Button";
import { Dialog } from "@components/Dialog/Dialog";
import { pushToast } from "@/components/toast/toastStore";
import { fazerBackupAgora, getBackupDestination, getLastAutoBackupAt } from "@core/autoBackup";
import { normalizeBinding } from "@core/keymap";
import { notasEmTexto } from "@/core/updater/notasTexto";
import {
  instalarAtualizacao,
  mostrarNovidades,
  useUpdaterStore,
  verificarAtualizacao,
} from "@/core/updater/updaterStore";
import { resolveBinding, useKeymapStore } from "@stores/keymapStore";
import { version as VERSAO } from "../../../package.json";
import { estadoBackup, rotuloBackup } from "./casosHome";
import { useDialogosCaso } from "./dialogosStore";
import { escolherNotas, resumoNotas, semDownloads, versaoCurta } from "./notasVersao";
import styles from "./LateralHome.module.css";

// Lazy: as notas só entram em memória quando alguém abre o "O que mudou".
const NOTAS = import.meta.glob<string>("../../../docs/releases/*.md", { query: "?raw", import: "default" });

interface Props {
  totalCasos: number;
  onBrowse: () => void;
  onImport: () => void;
}

export function LateralHome({ totalCasos, onBrowse, onImport }: Props) {
  const abrirNovo = useDialogosCaso((s) => s.abrirNovo);
  return (
    <aside className={styles.lateral} aria-label="Ações e avisos">
      <button type="button" className={styles.novo} onClick={abrirNovo}>
        <Plus size={20} strokeWidth={2.5} aria-hidden />
        Novo caso
      </button>
      <div className={styles.duo}>
        <button type="button" className={styles.acao} onClick={onBrowse}>
          <FolderOpen size={18} aria-hidden />
          Abrir pasta .sicro
        </button>
        <button type="button" className={styles.acao} onClick={onImport}>
          <Download size={18} aria-hidden />
          Importar .sicroapp
        </button>
      </div>
      <CartaoVersao />
      <CartaoBackup totalCasos={totalCasos} />
      <CartaoAtalhos />
    </aside>
  );
}

function CartaoVersao() {
  const s = useUpdaterStore();
  const [notasAbertas, setNotasAbertas] = useState(false);
  const nova = s.status === "available" || s.status === "downloading" || s.status === "installed";
  const baixando = s.status === "downloading";
  const temNotas = escolherNotas(Object.keys(NOTAS), VERSAO) !== null;

  if (nova) {
    const resumo = resumoNotas(s.notas);
    return (
      <section className={`${styles.cartao} ${styles.cartaoNovo}`} aria-label="Atualização">
        <div className={styles.versaoTitulo}>
          <RefreshCw size={16} aria-hidden />
          SICRO {versaoCurta(s.nova)} disponível
        </div>
        {resumo && <p className={styles.resumo}>{resumo}</p>}
        {baixando ? (
          <span className={styles.miudo}>Baixando… {Math.round(s.progresso * 100)}%</span>
        ) : s.status === "installed" ? (
          <span className={styles.miudo}>Instalado. Reiniciando…</span>
        ) : (
          <div className={styles.botoes}>
            <Button variant="primary" size="sm" onClick={() => void instalarAtualizacao()}>
              {s.instalavel ? "Atualizar agora" : "Abrir download"}
            </Button>
            <Button variant="secondary" size="sm" onClick={mostrarNovidades}>
              Ver novidades
            </Button>
          </div>
        )}
      </section>
    );
  }

  const verificando = s.status === "checking";
  return (
    <section className={styles.cartao} aria-label="Versão">
      <span className={styles.rotulo}>
        Versão · <span className={styles.mono}>{VERSAO}</span>
        {s.status === "none" && " · você está na mais recente"}
        {verificando && " · verificando…"}
      </span>
      <div className={styles.links}>
        {temNotas && (
          <button type="button" className={styles.link} onClick={() => setNotasAbertas(true)}>
            O que mudou na {versaoCurta(VERSAO)}
          </button>
        )}
        {!verificando && s.status !== "none" && (
          <button type="button" className={styles.link} onClick={() => void verificarAtualizacao(false)}>
            Verificar atualizações
          </button>
        )}
      </div>
      {notasAbertas && <NotasDialog onClose={() => setNotasAbertas(false)} />}
    </section>
  );
}

function NotasDialog({ onClose }: { onClose: () => void }) {
  const [texto, setTexto] = useState<string | null>(null);
  const escolha = escolherNotas(Object.keys(NOTAS), VERSAO);

  useEffect(() => {
    if (!escolha) return;
    let cancelado = false;
    void NOTAS[escolha.chave]!()
      .then((md) => {
        if (!cancelado) setTexto(notasEmTexto(semDownloads(md)));
      })
      .catch(() => {
        if (!cancelado) setTexto("");
      });
    return () => {
      cancelado = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Dialog open title={`SICRO ${versaoCurta(escolha?.versao ?? VERSAO)}`} onClose={onClose}>
      <pre className={styles.notas}>{texto ?? "Carregando…"}</pre>
    </Dialog>
  );
}

function CartaoBackup({ totalCasos }: { totalCasos: number }) {
  const navigate = useNavigate();
  const [ultimo, setUltimo] = useState<string | null>(getLastAutoBackupAt);
  const [fazendo, setFazendo] = useState(false);
  const destino = getBackupDestination();
  const agora = new Date();
  const estado = estadoBackup(ultimo, agora);

  const fazer = async () => {
    if (fazendo) return;
    setFazendo(true);
    try {
      const r = await fazerBackupAgora();
      if (r === "sem-destino") {
        pushToast("warn", "Escolha a pasta de backup em Configurações.", { title: "Backup" });
        navigate("/configuracoes");
      }
      setUltimo(getLastAutoBackupAt());
    } finally {
      setFazendo(false);
    }
  };

  return (
    <section className={styles.cartao} aria-label="Backup">
      <span className={styles.rotulo}>Backup</span>
      <div className={styles.linhaEntre}>
        <span className={styles.texto}>
          {ultimo ? `Último: ${rotuloBackup(ultimo, agora)}` : "Nenhum backup ainda"}
        </span>
        <span className={estado === "em-dia" ? styles.ok : styles.alerta}>
          ● {estado === "em-dia" ? "em dia" : estado === "atrasado" ? "atrasado" : "pendente"}
        </span>
      </div>
      <span className={styles.miudo}>
        {totalCasos} {totalCasos === 1 ? "caso" : "casos"}
        {destino ? ` · ${destino}` : " · sem pasta de backup"}
      </span>
      <button type="button" className={styles.link} onClick={() => void fazer()} disabled={fazendo}>
        {fazendo ? "Fazendo backup…" : "Fazer backup agora"}
      </button>
    </section>
  );
}

// Os de módulo só entram se existirem no catálogo (ids estáveis em keymap/*.ts).
const ATALHOS: Array<{ id: string; label: string }> = [
  { id: "geral.trocarCaso", label: "Trocar de caso" },
  { id: "geral.novoCaso", label: "Novo caso" },
  { id: "croqui.save", label: "Salvar croqui" },
  { id: "imagem.save", label: "Salvar análise de imagem" },
  { id: "video.gotoTime", label: "Ir para tempo no vídeo" },
];

function CartaoAtalhos() {
  const overrides = useKeymapStore((s) => s.overrides);
  return (
    <section className={styles.cartao} aria-label="Atalhos">
      <span className={styles.rotulo}>Atalhos</span>
      {ATALHOS.map((a) => {
        const b = normalizeBinding(resolveBinding(overrides, a.id));
        if (!b) return null;
        return (
          <div key={a.id} className={styles.linhaEntre}>
            <span className={styles.atalhoNome}>{a.label}</span>
            <kbd className={styles.kbd}>{b}</kbd>
          </div>
        );
      })}
    </section>
  );
}
