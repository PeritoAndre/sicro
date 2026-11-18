/** Som do clipe (P1.5): chaves do volume (P4.2: a raia "Volume" com "Chave no cursor" / "Tirar chave", tempo de
 *  fonte; o valor no cursor é o "Som" do trecho) e "Silenciar trecho" (Tom, Mudo) · "Da degravação…" · "Motivo". Os
 *  silêncios valem em tempo de fonte; o que vem da degravação é o trecho de fala cortado no clipe. A prévia viva toca
 *  o som da fonte: chaves e silêncios levam o ícone de estado (§8.6). */

import { useState } from "react";
import { BellOff, Captions, ChevronDown, ChevronRight, EarOff, Trash2, VolumeX } from "lucide-react";
import type { ClipeTrecho, Id, Seg, Silencio, TrechoFala } from "@domain/edicao";
import { commands } from "@core/commands";
import { pushToast } from "@components/toast/toastStore";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { separarErro, useEdicaoStore } from "../../store/edicaoStore";
import { novoId } from "../../modelo/projeto";
import { mudar, noClipe, tempoNoClipe } from "../acoes";
import { Escolha, Grade, Icone, Mini, Secao, Tempo, Texto } from "../campos";
import { CurvasDoItem } from "../../linha/curvas/CurvasDoItem";
import { passaDeUm } from "../../linha/curvas/chaves";
import { useShaDe } from "../contexto";
import type { SecaoProps } from "../registro";
import styles from "../Inspetor.module.css";

export const AUDIO_CLIPE_PRONTO = true;

/** Silêncio novo no cursor: 1 s, ou o que couber até o fim do trecho. */
const DURACAO_NOVO = 1;
const MODOS = [
  { v: "tom" as const, r: "Tom" },
  { v: "mudo" as const, r: "Mudo" },
];

const noTrecho = (id: Id, f: (c: ClipeTrecho) => void) => noClipe<ClipeTrecho>(id, (x) => f(x as ClipeTrecho));
const noSilencio = (clipe: Id, id: Id, f: (s: Silencio) => void) =>
  noTrecho(clipe, (x) => {
    const s = x.audio.silencios.find((y) => y.id === id);
    if (s) f(s);
  });

/** Falas da degravação que tocam no trecho, cortadas nele (tempo de fonte). */
export function falasDoTrecho(falas: readonly TrechoFala[], de: Seg, ate: Seg): TrechoFala[] {
  return falas.filter((f) => f.ate > de && f.de < ate && f.ate > f.de).map((f) => ({ ...f, de: Math.max(f.de, de), ate: Math.min(f.ate, ate) }));
}

/** Estado "só no render", sem texto. */
const SoNoRender = ({ de }: { de: string }) => (
  <span className={styles.dim} data-estado="so-no-render" data-de={de} aria-hidden>
    <EarOff size={12} />
  </span>
);

const mesmo = (a: Seg, b: Seg) => Math.abs(a - b) < 1e-6;
/** O silêncio que veio desta fala, se há. */
export const silencioDaFala = (silencios: readonly Silencio[], f: TrechoFala) => silencios.find((s) => mesmo(s.de, f.de) && mesmo(s.ate, f.ate)) ?? null;

export function AudioClipeSecao({ clipe: c }: SecaoProps<ClipeTrecho>) {
  const id = c.id;
  const ws = useEdicaoStore((s) => s.ws);
  const so = useEdicaoStore((s) => s.soLeitura);
  const sha = useShaDe(c.midia);
  const [aberto, setAberto] = useState<Id | null>(null);
  const [falas, setFalas] = useState<{ midia: Id; lista: TrechoFala[] } | null>(null);
  const dentro = (t: Seg) => t >= c.inicio_s - 1e-9 && t <= c.fim_s + 1e-9;

  const silenciar = () => {
    const de = Math.min(Math.max(tempoNoClipe(c), c.inicio_s), c.fim_s);
    const novo: Silencio = { id: novoId(), de, ate: Math.min(de + DURACAO_NOVO, c.fim_s), modo: "tom", motivo: "" };
    if (novo.ate <= novo.de) return;
    mudar("Silenciar trecho", noTrecho(id, (x) => void x.audio.silencios.push(novo)));
    setAberto(novo.id);
  };
  const daDegravacao = async () => {
    if (falas?.midia === c.midia) return setFalas(null);
    if (!ws || !sha) return;
    try {
      const d = await commands.edicaoDegravacao(ws, sha);
      setFalas({ midia: c.midia, lista: d.trechos });
    } catch (e) {
      pushToast("error", separarErro(e).frase);
    }
  };
  // a fala escolhida vira silêncio (Tom); escolhida de novo, sai
  const alternarFala = (f: TrechoFala) =>
    mudar(
      "Da degravação…",
      noTrecho(id, (x) => {
        const ja = silencioDaFala(x.audio.silencios, f);
        if (ja) x.audio.silencios = x.audio.silencios.filter((s) => s.id !== ja.id);
        else x.audio.silencios.push({ id: novoId(), de: f.de, ate: f.ate, modo: "tom", motivo: "" });
      }),
    );
  const lista = falas?.midia === c.midia ? falasDoTrecho(falas.lista, c.inicio_s, c.fim_s) : null;

  return (
    <Secao
      titulo="Som"
      acoes={passaDeUm(c.audio.volume) ? <SoNoRender de="volume" /> : undefined}
    >
      <CurvasDoItem item={{ k: "som", id }} props={["volume"]} />
      <div className={styles.barra}>
        <Mini data-acao="silenciar" disabled={c.audio.mudo} onClick={silenciar}>
          <BellOff size={12} /> Silenciar trecho
        </Mini>
        <Mini data-acao="da-degravacao" ativo={lista !== null} disabled={c.audio.mudo || !sha} onClick={() => void daDegravacao()}>
          <Captions size={12} /> Da degravação…
        </Mini>
      </div>
      {lista && lista.length > 0 && (
        <ul className={styles.efeitos} data-falas>
          {lista.map((f) => {
            const ja = silencioDaFala(c.audio.silencios, f) !== null;
            return (
              <li key={`${f.de}:${f.ate}`} className={styles.efeito}>
                <button type="button" className={styles.efeitoNome} aria-pressed={ja} data-fala={f.de} disabled={so} onClick={() => alternarFala(f)}>
                  {ja ? <VolumeX size={13} aria-hidden /> : <span className={styles.semSeta} aria-hidden />}
                  <span className={styles.mono} data-dado>{formatarTempo(f.de)}</span>
                  <span className={styles.cortaTexto} data-dado>{f.texto}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {c.audio.silencios.length > 0 && (
        <ul className={styles.efeitos}>
          {c.audio.silencios.map((s) => {
            const abre = aberto === s.id;
            return (
              <li key={s.id} className={`${styles.efeito} ${abre ? styles.efeitoAberto : ""}`} data-silencio={s.id}>
                <div className={styles.efeitoTopo}>
                  <button type="button" className={styles.efeitoNome} aria-expanded={abre} data-acao="abrir-silencio" onClick={() => setAberto(abre ? null : s.id)}>
                    {abre ? <ChevronDown size={12} aria-hidden /> : <ChevronRight size={12} aria-hidden />}
                    {s.modo === "tom" ? <BellOff size={13} aria-hidden /> : <VolumeX size={13} aria-hidden />}
                    <span className={styles.mono} data-dado>{formatarTempo(s.de)}</span>
                    <span className={styles.dim} data-dado>{formatarTempo(s.ate)}</span>
                  </button>
                  <span className={styles.acoes}>
                    <SoNoRender de="silencio" />
                    <Icone rotulo="Remover" onClick={() => mudar("Remover", noTrecho(id, (x) => void (x.audio.silencios = x.audio.silencios.filter((y) => y.id !== s.id))))}>
                      <Trash2 size={13} />
                    </Icone>
                  </span>
                </div>
                {abre && (
                  <Grade>
                    <Tempo rotulo="De" item={s.id} base="fonte" valor={s.de} valido={(t) => t < s.ate && dentro(t)} aplicar={(t) => noSilencio(id, s.id, (x) => void (x.de = t))} />
                    <Tempo rotulo="Até" item={s.id} base="fonte" valor={s.ate} valido={(t) => t > s.de && dentro(t)} aplicar={(t) => noSilencio(id, s.id, (x) => void (x.ate = t))} />
                    <Escolha rotulo="Silenciar trecho" semRotulo larga valor={s.modo} opcoes={MODOS} aplicar={(v) => noSilencio(id, s.id, (x) => void (x.modo = v))} />
                    <Texto rotulo="Motivo" larga valor={s.motivo} aplicar={(v) => noSilencio(id, s.id, (x) => void (x.motivo = v))} />
                  </Grade>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </Secao>
  );
}
