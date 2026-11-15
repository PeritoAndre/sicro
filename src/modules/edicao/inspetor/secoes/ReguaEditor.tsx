/** Régua (P1.3): "Calibração" · "Comprimento (m)" · "Divisões" · "Direção" · "Cor" · "Espessura" · "Contorno" · "Rótulo";
 *  âncora no chão por "Marcar". O retrato da calibração vem do Rust (`edicaoRetratoCalibracao`), no espaço do PNG que
 *  a análise coletou; a lista é a das calibrações da mídia do clipe, mais nova primeiro. */

import { ArrowLeft, ArrowRight, Crosshair, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import type { Draft } from "immer";
import { commands } from "@core/commands";
import { pushToast } from "@components/toast/toastStore";
import type { ClipePrincipal, ClipeSobreposto, ContextoClipe, EfRegua, Efeito, Id, Ponto, ProjetoEdicao, RetratoCalibracao } from "@domain/edicao";
import type { VideoSpeedCalibration } from "@domain/video_speed";
import { describeCalibration } from "@modules/video/editor/medir/ReferenceGuide";
import { separarErro, useEdicaoStore } from "../../store/edicaoStore";
import { acharClipe, acharEfeito, acharMidia, novoId } from "../../modelo/projeto";
import { pontasConferidas } from "../../palco/desenho/medidas";
import { mudar, noClipe, noEfeito, selecionarEfeito } from "../acoes";
import { Cor, Escolha, Grade, Icone, Marca, Mini, Numero } from "../campos";
import { pedirMarcacao } from "../contexto";
import { COR_DESENHO } from "../novos";
import type { EditorEfeitoProps } from "../registro";
import { useInspetorUi } from "../ui";
import { round } from "./comum";
import styles from "../Inspetor.module.css";

export const REGUA_PRONTA = true;

type Clipe = ClipePrincipal | ClipeSobreposto;

/** SHA-256 do vídeo do clipe (trecho e quadro congelado): as análises se ligam à mídia por ele. */
export function shaDoClipe(d: ProjetoEdicao, c: Clipe): string | null {
  return c.tipo === "trecho" || c.tipo === "congelar" ? (acharMidia(d, c.midia)?.sha256 ?? null) : null;
}

/** Lista que vem do backend, pedida de novo quando a chave ou a `vez` muda (a análise fica montada ao lado e pode
 *  ter criado outra); null enquanto a da chave não chegou. Falha = vazia. */
export function useLista<T>(chave: string | null, buscar: () => Promise<T[]>, vez = 0): T[] | null {
  const [lista, setLista] = useState<{ chave: string; itens: T[] } | null>(null);
  useEffect(() => {
    if (!chave) return;
    let vivo = true;
    buscar().then(
      (itens) => vivo && setLista({ chave, itens }),
      () => vivo && setLista({ chave, itens: [] }),
    );
    return () => {
      vivo = false;
    };
  }, [chave, vez]); // eslint-disable-line react-hooks/exhaustive-deps
  return chave && lista?.chave === chave ? lista.itens : null;
}

/** Retrato pelo comando; falha = aviso passageiro com a frase do catálogo (`calibracao_ausente`, `calculo_ausente`). */
export async function pedirRetrato<T>(pedir: (ws: string) => Promise<T>): Promise<T | null> {
  const ws = useEdicaoStore.getState().ws;
  if (!ws) return null;
  try {
    return await pedir(ws);
  } catch (e) {
    pushToast("error", separarErro(e).frase);
    return null;
  }
}

/** Lista de dados (as opções são dados da análise, não rótulos). */
export function ListaDeDados({ rotulo, valor, opcoes, aplicar }: { rotulo: string; valor: string; opcoes: { v: string; r: string }[]; aplicar: (v: string) => void }) {
  const so = useEdicaoStore((s) => s.soLeitura || !s.doc);
  return (
    <label className={`${styles.campo} ${styles.larga}`}>
      <span className={styles.rotulo}>{rotulo}</span>
      <select className={styles.entrada} value={valor} disabled={so} data-dado onChange={(e) => e.target.value !== valor && aplicar(e.target.value)}>
        {!opcoes.some((o) => o.v === valor) && <option value={valor} />}
        {opcoes.map((o) => (
          <option key={o.v} value={o.v}>
            {o.r}
          </option>
        ))}
      </select>
    </label>
  );
}

const dataHora = (iso: string) => {
  const d = new Date(iso);
  const p = (v: number) => String(v).padStart(2, "0");
  return Number.isNaN(d.getTime()) ? iso : `${p(d.getDate())}/${p(d.getMonth() + 1)}/${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}`;
};

/** Grava no efeito o que chegou do backend, se ele ainda existe (o perito pode ter removido durante a espera). */
export function noEfeitoSeExiste<E extends Efeito>(id: Id, rotulo: string, f: (e: Draft<E>) => void): void {
  const d = useEdicaoStore.getState().doc;
  if (d && acharEfeito(d, id)) mudar(rotulo, noEfeito<E>(id, f));
}

/** Centro da região calibrada (média dos pontos): dentro do polígono do plano e no meio do segmento 1D. */
function centro(regiao: Ponto[]): Ponto {
  if (regiao.length === 0) return { x: 0.5, y: 0.5 };
  return { x: regiao.reduce((s, p) => s + p.x, 0) / regiao.length, y: regiao.reduce((s, p) => s + p.y, 0) / regiao.length };
}

const RETRATO_VAZIO: RetratoCalibracao = { id: "", metodo: "plane", h: [1, 0, 0, 0, 1, 0, 0, 0, 1], espaco: { w: 0, h: 0 }, pontos_sha256: "", residuo_m: 0, regiao: [] };

function reguaCom(r: RetratoCalibracao): EfRegua {
  return {
    id: novoId(),
    ligado: true,
    nome: "",
    ativo: null,
    tipo: "regua",
    calibracao: r,
    ancora: centro(r.regiao),
    direcao_graus: 0,
    comprimento_m: 1,
    // 1 m mede dezenas de px numa câmera comum: mais divisões que isso viram um bloco
    divisoes: 2,
    traco: { cor: COR_DESENHO, espessura: 0.004, contorno: true },
    rotulo: true,
  };
}

/** Padrões que cabem na região calibrada (o desenho recusa régua fora dela): do centro a 0° ou a 90°; senão do
 *  primeiro ponto da região a 0°. Nenhum cabe: os do centro, com o erro à vista. */
function reguaQueCabe(r: RetratoCalibracao): EfRegua {
  const base = reguaCom(r);
  const tentativas: Partial<EfRegua>[] = [{}, { direcao_graus: 90 }, ...(r.regiao[0] ? [{ ancora: r.regiao[0] }] : [])];
  for (const t of tentativas) {
    const e = { ...base, ...t };
    try {
      pontasConferidas(e);
      return e;
    } catch {
      // tenta a seguinte
    }
  }
  return base;
}

/** Sentido da régua no modelo 1D (`cross_ratio`): só o sinal do cosseno vale (a conta do Rust). */
const sentido = (graus: number) => (Math.cos((graus * Math.PI) / 180) >= -1e-12 ? 0 : 180);

export function ReguaEditor({ valor: e, ctx }: EditorEfeitoProps<EfRegua>) {
  const id = e.id;
  const em = (f: (x: Draft<EfRegua>) => void) => noEfeito<EfRegua>(id, f);
  const sha = shaDoClipe(ctx.projeto, ctx.clipe);
  const [vez, setVez] = useState(0);
  const cals = useLista<VideoSpeedCalibration>(sha ? `${ctx.ws}|${sha}` : null, () => commands.listSpeedCalibrations(ctx.ws, sha!), vez);
  const chave = useInspetorUi((s) => s.chave);
  const trocar = async (calId: string) => {
    const r = await pedirRetrato((ws) => commands.edicaoRetratoCalibracao(ws, calId));
    if (r) noEfeitoSeExiste<EfRegua>(id, "Calibração", (x) => void (x.calibracao = r));
  };
  const umaDimensao = e.calibracao.metodo === "cross_ratio";
  return (
    <Grade>
      <div className={`${styles.larga} ${styles.linhaCor}`}>
        <ListaDeDados
          rotulo="Calibração"
          valor={e.calibracao.id}
          opcoes={(cals ?? []).map((c) => ({ v: c.id, r: `${describeCalibration(c)} · ${dataHora(c.created_at)}` }))}
          aplicar={(v) => void trocar(v)}
        />
        <Icone
          rotulo="Calibração"
          data-acao="retrato"
          onClick={() => {
            setVez((v) => v + 1);
            void trocar(e.calibracao.id);
          }}
        >
          <RefreshCw size={13} />
        </Icone>
      </div>
      <Numero rotulo="Comprimento (m)" item={id} valor={e.comprimento_m} min={0.01} max={1000} passo={0.5} casas={2} aplicar={(v) => em((x) => void (x.comprimento_m = v))} />
      <Numero rotulo="Divisões" item={id} valor={e.divisoes} min={0} max={100} passo={1} casas={0} aplicar={(v) => em((x) => void (x.divisoes = Math.round(v)))} />
      {umaDimensao ? (
        <Escolha
          rotulo="Direção"
          valor={sentido(e.direcao_graus)}
          opcoes={[
            { v: 0, icone: <ArrowRight size={13} /> },
            { v: 180, icone: <ArrowLeft size={13} /> },
          ]}
          aplicar={(v) => em((x) => void (x.direcao_graus = v))}
        />
      ) : (
        <Numero rotulo="Direção" item={id} valor={e.direcao_graus} min={-360} max={360} passo={15} casas={1} unidade="°" aplicar={(v) => em((x) => void (x.direcao_graus = v))} />
      )}
      <div>
        <Mini
          ativo={chave === `${id}:ancora`}
          data-acao="marcar-ancora"
          onClick={() => pedirMarcacao({ tipo: "ponto", efeito: id }, (v) => mudar("Marcar", em((x) => void (x.ancora = { x: v.x, y: v.y }))), `${id}:ancora`)}
        >
          <Crosshair size={12} /> Marcar
        </Mini>
      </div>
      <Cor rotulo="Cor" item={id} valor={e.traco.cor} aplicar={(v) => em((x) => void (x.traco.cor = v))} />
      <Numero
        rotulo="Espessura"
        item={id}
        valor={round(e.traco.espessura * 100, 2)}
        min={0.1}
        max={5}
        passo={0.1}
        casas={1}
        unidade="%"
        aplicar={(v) => em((x) => void (x.traco.espessura = v / 100))}
      />
      <Marca rotulo="Contorno" valor={e.traco.contorno} aplicar={(v) => em((x) => void (x.traco.contorno = v))} />
      <Marca rotulo="Rótulo" valor={e.rotulo} aplicar={(v) => em((x) => void (x.rotulo = v))} />
    </Grade>
  );
}

/** Régua nova com os padrões, sem calibração: quem cria pelo "Adicionar" põe o retrato (`adicionarRegua`). */
export function novaRegua(ctx: ContextoClipe): EfRegua {
  void ctx;
  return reguaCom(RETRATO_VAZIO);
}

/** "Adicionar → Régua": o retrato da calibração mais nova da mídia do clipe, a régua dentro da região calibrada.
 *  Sem calibração (ou clipe sem vídeo), o aviso do catálogo e nada criado. Devolve o id do efeito, já selecionado. */
export async function adicionarRegua(clipeId: Id, rotulo: string): Promise<Id | null> {
  const s = useEdicaoStore.getState();
  const c = s.doc ? acharClipe(s.doc, clipeId)?.clipe : null;
  if (!s.ws || !s.doc || !c) return null;
  const projeto = s.doc.id;
  const sha = shaDoClipe(s.doc, c);
  const ws = s.ws;
  const cals = sha ? await commands.listSpeedCalibrations(ws, sha).catch(() => [] as VideoSpeedCalibration[]) : [];
  // id vazio: o backend responde `calibracao_ausente` (a frase do catálogo vai para o aviso)
  const r = await pedirRetrato((w) => commands.edicaoRetratoCalibracao(w, cals[0]?.id ?? ""));
  const d = useEdicaoStore.getState().doc;
  if (!r || !d || d.id !== projeto || !acharClipe(d, clipeId)) return null;
  const ef = reguaQueCabe(r);
  mudar(rotulo, noClipe(clipeId, (x) => void x.efeitos.push(ef)));
  selecionarEfeito(ef.id);
  return ef.id;
}
