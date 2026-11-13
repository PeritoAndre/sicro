/** Diálogos globais de caso: busca rápida (Ctrl+O) e novo caso (Ctrl+N). Montados no AppShell. */

import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Search } from "lucide-react";
import { Button } from "@components/Button/Button";
import { Dialog } from "@components/Dialog/Dialog";
import { pushToast } from "@/components/toast/toastStore";
import { useNavGuard } from "@/app/navGuard";
import { toSicroError } from "@core/errors";
import { formatRelative } from "@core/formatters";
import type { RecentOccurrence } from "@domain/occurrence";
import { selectActiveWorkspacePath, selectRecents, useWorkspaceStore } from "@stores/workspaceStore";
import { casaBusca, estaConcluido, filtrarOrdenar } from "./casosHome";
import { useDialogosCaso } from "./dialogosStore";
import { useFixados } from "./fixadosStore";
import { tituloRecente, useAbrirCaso, useIndiceCasos } from "./useCasos";
import styles from "./CasosDialogos.module.css";

export function CasosDialogos() {
  return (
    <>
      <TrocarCasoDialog />
      <NovoCasoDialog />
    </>
  );
}

function TrocarCasoDialog() {
  const aberto = useDialogosCaso((s) => s.trocar);
  const fechar = useDialogosCaso((s) => s.fecharTrocar);
  if (!aberto) return null;
  return <BuscaCaso onClose={fechar} />;
}

function BuscaCaso({ onClose }: { onClose: () => void }) {
  const recents = useWorkspaceStore(selectRecents);
  const ativo = useWorkspaceStore(selectActiveWorkspacePath);
  const fixados = useFixados((s) => s.ids);
  const indice = useIndiceCasos();
  const titulo = (r: RecentOccurrence) => tituloRecente(r, indice);
  const abrirCaso = useAbrirCaso();
  const [busca, setBusca] = useState("");
  const [sel, setSel] = useState(0);
  const listaRef = useRef<HTMLUListElement>(null);

  const itens = useMemo(() => {
    const ordenados = filtrarOrdenar(recents, "todos", new Set(fixados));
    return ordenados.filter((r) => {
      const e = indice.get(r.workspace_path);
      return casaBusca(
        [r.occurrence_label, titulo(r), e?.numero_bo, e?.titulo, e?.municipio, e?.tipo_pericia],
        busca,
      );
    });
    // titulo() depende só do índice
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recents, fixados, indice, busca]);

  useEffect(() => setSel(0), [busca]);
  useEffect(() => {
    listaRef.current?.children[sel]?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const escolher = (r: RecentOccurrence) => {
    onClose();
    void abrirCaso(r.workspace_path);
  };

  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((i) => Math.min(i + 1, Math.max(itens.length - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const r = itens[sel];
      if (r) escolher(r);
    }
  };

  return (
    <Dialog open title={ativo ? "Trocar de caso" : "Abrir caso"} onClose={onClose}>
      <label className={styles.busca}>
        <Search size={15} aria-hidden />
        <input
          autoFocus
          type="text"
          value={busca}
          onChange={(e) => setBusca(e.target.value)}
          onKeyDown={onKey}
          placeholder="Buscar por nome, laudo ou ocorrência"
          aria-label="Buscar caso"
          spellCheck={false}
        />
      </label>
      {itens.length === 0 ? (
        <p className={styles.vazio}>{recents.length === 0 ? "Nenhum caso ainda." : "Nenhum caso encontrado."}</p>
      ) : (
        <ul className={styles.lista} ref={listaRef} role="listbox" aria-label="Casos">
          {itens.map((r, i) => (
            <li key={r.workspace_id} role="option" aria-selected={i === sel}>
              <button
                type="button"
                className={`${styles.item} ${i === sel ? styles.itemSel : ""}`}
                onMouseMove={() => setSel(i)}
                onClick={() => escolher(r)}
                title={r.workspace_path}
              >
                <span className={styles.nome}>{titulo(r)}</span>
                {r.workspace_path === ativo && <span className={styles.tag}>aberto</span>}
                {estaConcluido(r.status) && <span className={styles.tag}>concluído</span>}
                <span className={styles.quando}>{formatRelative(r.last_opened_at)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}

function NovoCasoDialog() {
  const aberto = useDialogosCaso((s) => s.novo);
  const fechar = useDialogosCaso((s) => s.fecharNovo);
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const createOccurrence = useWorkspaceStore((s) => s.createOccurrence);
  const [nome, setNome] = useState("");
  const [ocupado, setOcupado] = useState(false);

  useEffect(() => {
    if (aberto) setNome("");
  }, [aberto]);

  const criar = () => {
    if (ocupado) return;
    const executar = async () => {
      setOcupado(true);
      try {
        await createOccurrence({ titulo: nome.trim() || null });
        fechar();
        if (pathname !== "/") navigate("/");
      } catch (e) {
        pushToast("error", toSicroError(e).message);
      } finally {
        setOcupado(false);
      }
    };
    void useNavGuard.getState().attemptNavigation(() => void executar());
  };

  return (
    <Dialog
      open={aberto}
      title="Novo caso"
      onClose={fechar}
      footer={
        <>
          <Button variant="ghost" onClick={fechar}>
            Cancelar
          </Button>
          <Button variant="primary" onClick={criar} disabled={ocupado}>
            Criar caso
          </Button>
        </>
      }
    >
      <input
        autoFocus
        type="text"
        className={styles.nomeNovo}
        value={nome}
        onChange={(e) => setNome(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            criar();
          }
        }}
        placeholder="Nome do caso — ex.: Laudo 63404/26, Km 09 Duca Serra"
        aria-label="Nome do novo caso"
        disabled={ocupado}
        spellCheck={false}
      />
    </Dialog>
  );
}
