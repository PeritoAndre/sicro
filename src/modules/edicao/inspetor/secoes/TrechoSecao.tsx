/** Clipe · Trecho: nome, vídeo, início e fim (tempo de fonte, pts pela tabela), velocidade e som do clipe. Volume com
 *  chaves (P4.2): o "Som" mostra o do cursor e grava a chave dele. */

import { Volume2, VolumeX } from "lucide-react";
import type { ClipeTrecho } from "@domain/edicao";
import { avaliar, ehChaves } from "../../modelo/curva";
import { gravarEm } from "../../linha/curvas/chaves";
import { useInstanteDaChave } from "../../linha/curvas/CurvasDoItem";
import { noClipe } from "../acoes";
import { Deslizante, Grade, Marca, Secao, Tempo, Texto } from "../campos";
import { useEdicaoStore } from "../../store/edicaoStore";
import type { SecaoProps } from "../registro";
import { CorDaMidia, NomeMidia } from "./comum";
// P3.4: a velocidade (constante ou rampa) mora na seção dela
import { VelocidadeCampos } from "./VelocidadeSecao";
import styles from "../Inspetor.module.css";

export { velocidadeValida } from "./VelocidadeSecao";

export function TrechoSecao({ clipe: c, info, tabela }: SecaoProps<ClipeTrecho>) {
  const id = c.id;
  const so = useEdicaoStore((s) => s.soLeitura);
  const de = info?.de_s;
  const ate = info?.ate_s;
  const volume = c.audio.volume;
  const animado = ehChaves(volume);
  // com chaves: o instante do cursor no trecho (tempo de fonte), na conta da raia "Volume", e o valor da curva nele
  const tChave = useInstanteDaChave({ k: "som", id }, volume);
  const noCursor = animado && tChave !== null ? avaliar(volume, tChave) : null;
  // contratos (h) 17: #n e setas só com a tabela confiável; mídia não marcável não marca nem pelo texto
  const tab = info?.marcavel ? tabela : null;
  const travado = info?.marcavel === false;
  return (
    <Secao titulo="Trecho">
      <Grade>
        <Texto rotulo="Nome" valor={c.rotulo} larga aplicar={(v) => noClipe<ClipeTrecho>(id, (x) => void (x.rotulo = v))} />
        <NomeMidia rotulo="Vídeo" midia={c.midia} />
        <CorDaMidia midia={c.midia} info={info} />
        <Tempo
          rotulo="Início"
          item={id}
          valor={c.inicio_s}
          base="fonte"
          tabela={tab}
          desligado={travado}
          valido={(t) => t < c.fim_s && (de === undefined || t >= de - 1e-9)}
          aplicar={(t, pts) =>
            noClipe<ClipeTrecho>(id, (x) => {
              x.inicio_s = t;
              x.inicio_pts = pts;
            })
          }
        />
        <Tempo
          rotulo="Fim"
          item={id}
          valor={c.fim_s}
          base="fonte"
          tabela={tab}
          desligado={travado}
          valido={(t) => t > c.inicio_s && (ate === undefined || t <= ate + 1e-9)}
          aplicar={(t, pts) =>
            noClipe<ClipeTrecho>(id, (x) => {
              x.fim_s = t;
              // no fim da mídia não há quadro j_out: pts nulo
              x.fim_pts = ate !== undefined && t >= ate - 1e-9 ? null : pts;
            })
          }
        />
        <VelocidadeCampos clipe={c} tabela={tab} travado={travado} />
        <div className={styles.som}>
          <span className={styles.rotulo}>Som</span>
          <span className={styles.somLinha}>
            <button
              type="button"
              className={`${styles.icone} ${c.audio.mudo ? styles.iconeOn : ""}`}
              aria-pressed={c.audio.mudo}
              aria-label="Som"
              title="Som"
              disabled={so}
              onClick={() => useEdicaoStore.getState().alterar("Som", noClipe<ClipeTrecho>(id, (x) => void (x.audio.mudo = !x.audio.mudo)))}
            >
              {c.audio.mudo ? <VolumeX size={14} /> : <Volume2 size={14} />}
            </button>
            <Deslizante
              rotulo="Som"
              semRotulo
              item={id}
              valor={noCursor ?? (volume as number)}
              desligado={c.audio.mudo || (animado && tChave === null)}
              min={0}
              max={2}
              passo={0.05}
              mostrar={(v) => `${Math.round(v * 100)} %`}
              aplicar={(v) => noClipe<ClipeTrecho>(id, (x) => void (x.audio.volume = animado ? gravarEm(volume, tChave ?? c.inicio_s, v, null) : v))}
            />
          </span>
        </div>
        <Marca rotulo="Manter o tom" larga valor={c.audio.manter_tom} aplicar={(v) => noClipe<ClipeTrecho>(id, (x) => void (x.audio.manter_tom = v))} />
      </Grade>
    </Secao>
  );
}
