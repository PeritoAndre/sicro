/** Aba Áudio (P1.5): "Volume do original" · "Abaixar sob a narração" · "Normalizar" · "Limitador" · trilhas "Narração",
 *  "Música", "Efeitos" · "Importar…" · "Gravar" · "Parar" · "Preso a" · "Posição" · "De" · "Até" · "Entrada (s)" ·
 *  "Saída (s)". As trilhas e a gravação são as peças da `AudioSecao` (mesmos `data-acao`). Abaixar, normalização e
 *  limitador ligados levam o ícone de estado: a prévia viva não os aplica (§7.4, §8.6). */

import { EarOff, Upload } from "lucide-react";
import type { Draft } from "immer";
import type { Id, MidiaRef, ProjetoEdicao, TrilhaAudio } from "@domain/edicao";
import { useEdicaoStore } from "../store/edicaoStore";
import { clipeConhecido } from "../modelo/projeto";
import { juntarNarracao, mudar } from "../inspetor/acoes";
import { Deslizante, Grade, Marca, Mini, Secao } from "../inspetor/campos";
import { novaTrilhaNarracao } from "../inspetor/novos";
import { Gravador, PAPEL, Trilha } from "../inspetor/secoes/AudioSecao";
import { escolherMidia } from "../inspetor/secoes/importar";
import styles from "../inspetor/Inspetor.module.css";

export const PAINEL_AUDIO_PRONTO = true;

/** Alvo de projeto novo (§2.4; `receitas-audio-saida.md` §1). */
export const NORMALIZAR_PADRAO = { lufs: -16, pico_db: -1.5 } as const;

const PAPEIS: TrilhaAudio["papel"][] = ["narracao", "musica", "efeitos"];

/** Estado "só no render" ao lado da marca, sem texto. */
const SoNoRender = ({ de }: { de: string }) => (
  <span className={styles.dim} data-estado="so-no-render" data-de={de} aria-hidden>
    <EarOff size={12} />
  </span>
);

/** Clipe de áudio novo na trilha do papel (criada se não há), preso ao clipe em foco: um passo. */
export function importarNaTrilha(midia: MidiaRef, papel: TrilhaAudio["papel"], presoA: Id | null, rotulo: string): void {
  mudar(rotulo, (d: Draft<ProjetoEdicao>) => {
    let t = d.audios.find((x) => x.papel === papel);
    if (!t) {
      d.audios.push({ ...novaTrilhaNarracao(), papel });
      t = d.audios[d.audios.length - 1]!;
    }
    juntarNarracao(d, midia, presoA, t.id);
  });
}

export function PainelAudio() {
  const doc = useEdicaoStore((s) => s.doc);
  const foco = useEdicaoStore((s) => s.selecao.foco);
  if (!doc) return null;
  // clipe de áudio novo fica preso ao clipe da principal em foco
  const presoA = foco?.k === "clipe" && doc.principal.some((c) => clipeConhecido(c) && c.id === foco.id) ? foco.id : null;
  const narracoes = doc.audios.filter((t) => t.papel === "narracao");
  const mix = doc.mixagem;
  const abaixa = narracoes.length > 0 && narracoes.every((t) => t.abaixa_original);
  return (
    <>
      <Secao>
        <Grade>
          <Deslizante
            rotulo="Volume do original"
            larga
            item="mixagem"
            valor={mix.volume_original}
            min={0}
            max={2}
            passo={0.05}
            mostrar={(v) => `${Math.round(v * 100)} %`}
            aplicar={(v) => (d) => void (d.mixagem.volume_original = v)}
          />
          <span className={`${styles.somLinha} ${styles.larga}`}>
            <Marca
              rotulo="Abaixar sob a narração"
              valor={abaixa}
              desligado={narracoes.length === 0}
              aplicar={(v) => (d) => {
                for (const t of d.audios) if (t.papel === "narracao") t.abaixa_original = v;
              }}
            />
            {abaixa && <SoNoRender de="abaixar" />}
          </span>
          <span className={styles.somLinha}>
            <Marca rotulo="Normalizar" valor={mix.normalizar !== null} aplicar={(v) => (d) => void (d.mixagem.normalizar = v ? { ...NORMALIZAR_PADRAO } : null)} />
            {mix.normalizar !== null && <SoNoRender de="normalizar" />}
          </span>
          <span className={styles.somLinha}>
            <Marca rotulo="Limitador" valor={mix.limitador} aplicar={(v) => (d) => void (d.mixagem.limitador = v)} />
            {mix.limitador && <SoNoRender de="limitador" />}
          </span>
        </Grade>
      </Secao>
      {PAPEIS.map((papel) => {
        const trilhas = doc.audios.filter((t) => t.papel === papel);
        return (
          <div key={papel} data-papel={papel}>
            {trilhas.map((t) => (
              <Trilha key={t.id} t={t} principal={doc.principal} focoAudio={foco?.k === "audio" ? foco.id : null} />
            ))}
            <Secao titulo={trilhas.length === 0 ? PAPEL[papel] : undefined}>
              <div className={styles.barra}>
                <Mini data-acao="importar" onClick={() => void escolherMidia("audio").then((m) => m && importarNaTrilha(m, papel, presoA, "Importar…"))}>
                  <Upload size={12} /> Importar…
                </Mini>
                {papel === "narracao" && <Gravador presoA={presoA} trilha={narracoes[0]?.id} />}
              </div>
            </Secao>
          </div>
        );
      })}
    </>
  );
}
