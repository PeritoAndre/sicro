/** Quadro congelado e imagem na principal; imagem sobreposta. */

import type { ClipeCongelar, ClipeImagem, SobImagem } from "@domain/edicao";
import { noClipe } from "../acoes";
import { Grade, Marca, Numero, Secao, Tempo, Texto } from "../campos";
import type { SecaoProps } from "../registro";
import { NomeMidia, round } from "./comum";

const duracaoValida = (v: number) => v > 0 && Number.isFinite(v);

export function CongelarSecao({ clipe: c, info, tabela }: SecaoProps<ClipeCongelar>) {
  const id = c.id;
  return (
    <Secao titulo="Quadro congelado">
      <Grade>
        <Texto rotulo="Nome" larga valor={c.rotulo} aplicar={(v) => noClipe<ClipeCongelar>(id, (x) => void (x.rotulo = v))} />
        <NomeMidia rotulo="Vídeo" midia={c.midia} />
        <Tempo
          rotulo="Posição"
          item={id}
          valor={c.instante_s}
          base="fonte"
          tabela={info?.marcavel ? tabela : null}
          desligado={info?.marcavel === false}
          valido={(t) => !info || (t >= info.de_s - 1e-9 && t < info.ate_s)}
          aplicar={(t, pts) =>
            noClipe<ClipeCongelar>(id, (x) => {
              x.instante_s = t;
              x.instante_pts = pts;
            })
          }
        />
        <Numero rotulo="Duração (s)" item={id} valor={c.duracao_s} min={0.1} max={3600} passo={0.5} casas={2} valido={duracaoValida} aplicar={(v) => noClipe<ClipeCongelar>(id, (x) => void (x.duracao_s = v))} />
      </Grade>
    </Secao>
  );
}

export function ImagemClipeSecao({ clipe: c }: SecaoProps<ClipeImagem>) {
  const id = c.id;
  return (
    <Secao titulo="Imagem">
      <Grade>
        <Texto rotulo="Nome" larga valor={c.rotulo} aplicar={(v) => noClipe<ClipeImagem>(id, (x) => void (x.rotulo = v))} />
        <NomeMidia rotulo="Imagem" midia={c.midia} />
        <Numero rotulo="Duração (s)" item={id} valor={c.duracao_s} min={0.1} max={3600} passo={0.5} casas={2} valido={duracaoValida} aplicar={(v) => noClipe<ClipeImagem>(id, (x) => void (x.duracao_s = v))} />
      </Grade>
    </Secao>
  );
}

/** Imagem presa a um clipe ou ao programa; o lugar se ajusta pelas alças do palco. */
export function SobImagemSecao({ clipe: c }: SecaoProps<SobImagem>) {
  const id = c.id;
  const t = c.transformacao;
  const fixo = (v: unknown): v is number => typeof v === "number";
  return (
    <Secao titulo="Imagem">
      <Grade>
        <Texto rotulo="Nome" larga valor={c.rotulo} aplicar={(v) => noClipe<SobImagem>(id, (x) => void (x.rotulo = v))} />
        <NomeMidia rotulo="Imagem" midia={c.midia} />
        <Marca rotulo="Até o fim do clipe" larga valor={c.duracao_s === null} aplicar={(v) => noClipe<SobImagem>(id, (x) => void (x.duracao_s = v ? null : 3))} />
        {c.duracao_s !== null && (
          <Numero rotulo="Duração (s)" item={id} valor={c.duracao_s} min={0.1} max={3600} passo={0.5} casas={2} valido={duracaoValida} aplicar={(v) => noClipe<SobImagem>(id, (x) => void (x.duracao_s = v))} />
        )}
        <Numero
          rotulo="Tamanho"
          item={id}
          valor={fixo(t.escala) ? round(t.escala * 100, 1) : 100}
          desligado={!fixo(t.escala)}
          min={1}
          max={100}
          passo={1}
          casas={0}
          unidade="%"
          aplicar={(v) => noClipe<SobImagem>(id, (x) => void (x.transformacao.escala = v / 100))}
        />
      </Grade>
    </Secao>
  );
}
