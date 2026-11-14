/** Texto, carimbo de tempo ("Tempo") e imagem sobre o vídeo. Lugar pela grade de "Posição" ou marcado no palco. */

import { Crosshair, ImagePlus } from "lucide-react";
import type { Draft } from "immer";
import type { EfCarimbo, EfImagem, EfTexto, EstiloTexto, Lugar, ProjetoEdicao } from "@domain/edicao";
import { mudar, noEfeito } from "../acoes";
import { garantirMidia } from "../../modelo/projeto";
import { Grade, Lista, Mini, Numero, Posicao, Texto } from "../campos";
import { pedirMarcacao } from "../contexto";
import type { EditorEfeitoProps } from "../registro";
import { useInspetorUi } from "../ui";
import { EstiloCampos, NomeMidia, round } from "./comum";
import { escolherMidia } from "./importar";

/** "Marcar": o lugar vem do palco (ponto do canvas e o alinhamento atual). */
function MarcarLugar({ efeito, lugar, aplicar }: { efeito: string; lugar: Lugar; aplicar: (l: Lugar) => (d: Draft<ProjetoEdicao>) => void }) {
  const chave = useInspetorUi((s) => s.chave);
  const minha = `${efeito}:lugar`;
  return (
    <Mini
      ativo={chave === minha}
      data-acao="marcar-lugar"
      onClick={() =>
        pedirMarcacao(
          { tipo: "lugar", efeito },
          (v) => {
            const l = "h" in v ? (v as Lugar) : { ...lugar, x: (v as { x: number }).x, y: (v as { y: number }).y };
            mudar("Marcar", aplicar(l));
          },
          minha,
        )
      }
    >
      <Crosshair size={12} /> Marcar
    </Mini>
  );
}

function EstiloELugar<E extends EfTexto | EfCarimbo>({ e }: { e: E }) {
  const id = e.id;
  const emEstilo = (f: (x: Draft<EstiloTexto>) => void) => noEfeito<EfTexto | EfCarimbo>(id, (x) => f(x.estilo));
  const lugar = (l: Lugar) => noEfeito<EfTexto | EfCarimbo>(id, (x) => void (x.lugar = l));
  return (
    <>
      <EstiloCampos item={id} estilo={e.estilo} aplicar={emEstilo} />
      <Posicao rotulo="Posição" larga valor={e.lugar} aplicar={lugar} extra={<MarcarLugar efeito={id} lugar={e.lugar} aplicar={lugar} />} />
    </>
  );
}

export function TextoEditor({ valor: e }: EditorEfeitoProps<EfTexto>) {
  return (
    <Grade>
      <Texto rotulo="Texto" larga linhas={3} valor={e.conteudo} aplicar={(v) => noEfeito<EfTexto>(e.id, (x) => void (x.conteudo = v))} />
      <EstiloELugar e={e} />
    </Grade>
  );
}

const MOSTRA: { v: EfCarimbo["mostra"]; r: string }[] = [
  { v: "tempo_fonte", r: "Tempo do original" },
  { v: "relogio", r: "Relógio da câmera" },
  { v: "quadro", r: "Quadro" },
  { v: "tempo_programa", r: "Tempo do vídeo" },
];

export function CarimboEditor({ valor: e }: EditorEfeitoProps<EfCarimbo>) {
  return (
    <Grade>
      <Lista rotulo="Mostrar" larga valor={e.mostra} opcoes={MOSTRA} aplicar={(v) => noEfeito<EfCarimbo>(e.id, (x) => void (x.mostra = v))} />
      <EstiloELugar e={e} />
    </Grade>
  );
}

export function ImagemEditor({ valor: e }: EditorEfeitoProps<EfImagem>) {
  const id = e.id;
  const lugar = (l: Lugar) => noEfeito<EfImagem>(id, (x) => void (x.lugar = l));
  return (
    <Grade>
      <NomeMidia rotulo="Imagem" midia={e.midia} />
      <div>
        <Mini
          data-acao="trocar-imagem"
          onClick={() =>
            void escolherMidia("imagem").then((m) => {
              if (!m) return;
              mudar("Imagem…", (d) => {
                // o mesmo arquivo de novo não duplica a mídia (mesmo SHA-256)
                const mid = garantirMidia(d, m);
                noEfeito<EfImagem>(id, (x) => void (x.midia = mid))(d);
              });
            })
          }
        >
          <ImagePlus size={12} /> Imagem…
        </Mini>
      </div>
      <Numero rotulo="Tamanho" item={id} valor={round(e.largura * 100, 1)} min={1} max={100} passo={1} casas={1} unidade="%" aplicar={(v) => noEfeito<EfImagem>(id, (x) => void (x.largura = v / 100))} />
      <Posicao rotulo="Posição" larga valor={e.lugar} aplicar={lugar} extra={<MarcarLugar efeito={id} lugar={e.lugar} aplicar={lugar} />} />
    </Grade>
  );
}
