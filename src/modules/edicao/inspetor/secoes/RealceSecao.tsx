/** Clipe · Realce (P1.4): "Estabilizar" · "Suavizar" · "Borda" (Preta, Ampliar) · "Refazer estabilização" · "Lente" ·
 *  "Desentrelaçar" (Quadro, Campo) · "Brilho" · "Contraste" · "Gama" · "Saturação" · "Nitidez" · "Média de quadros".
 *  Uma linha por efeito de origem, na ordem da cadeia do motor; o que o clipe ainda não tem aparece desligado.
 *  O intermediário de origem (`edicaoOrigemPedir`) é pedido quando os efeitos ligados mudam e acompanhado pela fila
 *  de fundo; o `.trf` e o zoom que ele devolve são gravados no efeito por `alterar()`. Estado do intermediário: ícone
 *  no topo da seção, sem texto. */

import { useEffect, useState } from "react";
import { AlignJustify, Anchor, Aperture, CircleAlert, Eye, EyeOff, Focus, Layers, LoaderCircle, RotateCw, SlidersHorizontal, Trash2, type LucideIcon } from "lucide-react";
import type { Draft } from "immer";
import type { Desconhecido, EfAjustes, EfDesentrelacar, EfEstabilizar, Efeito, EfLente, EfMediaTemporal, EfNitidez, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";
import { commands } from "@core/commands";
import { askConfirm } from "@components/Dialog/ask";
import { useEdicaoStore } from "../../store/edicaoStore";
import { useOrigemStore, type OrigemDoClipe } from "../../store/origemStore";
import { ESPERA, esquecerOrigens, pedirDeNovo, useOrigem } from "../origemDoClipe";
import { efeitoConhecido, novoId } from "../../modelo/projeto";
import { campoCabe } from "../../modelo/realce";
import { faixaDoTrecho, ptsDaTabela } from "../../modelo/tempo";
import { ligarEfeito, marcacoesNoEspaco, mudar, MUDA_ESPACO, noClipe, noEfeito, removerEfeito, type ClipeQualquer } from "../acoes";
import { useInvalidos, useShaDe } from "../contexto";
import { Deslizante, Escolha, Icone, Mini, Numero, Secao } from "../campos";
import { descritor, type SecaoProps } from "../registro";
import styles from "../Inspetor.module.css";

export const REALCE_PRONTO = true;

type Realce = EfDesentrelacar | EfEstabilizar | EfLente | EfAjustes | EfNitidez | EfMediaTemporal;
type Tipo = Realce["tipo"];

/** Na ordem em que o motor aplica (§5.3). */
const TIPOS: { tipo: Tipo; rotulo: string; icone: LucideIcon }[] = [
  { tipo: "desentrelacar", rotulo: "Desentrelaçar", icone: AlignJustify },
  { tipo: "estabilizar", rotulo: "Estabilizar", icone: Anchor },
  { tipo: "lente", rotulo: "Lente", icone: Aperture },
  { tipo: "ajustes", rotulo: "Brilho", icone: SlidersHorizontal },
  { tipo: "nitidez", rotulo: "Nitidez", icone: Focus },
  { tipo: "media_temporal", rotulo: "Média de quadros", icone: Layers },
];
const ehRealce = (e: Efeito | Desconhecido): e is Realce => efeitoConhecido(e) && TIPOS.some((t) => t.tipo === e.tipo);

/** Quadros da média: os mesmos limites da validação. */
const MEDIA = { min: 2, max: 15, novo: 4 };
const SUAVIZAR = { min: 0, max: 100, novo: 15 };

// ---- intermediário de origem ----

// O pedido, o acompanhamento e a gravação do `.trf` moram em `inspetor/origemDoClipe.ts` (o palco e os cartões usam
// sem importar esta seção, que o registro importa). Daqui saem os mesmos nomes de antes.
export { ESPERA, esquecerOrigens, useOrigem, useOrigemStore, type OrigemDoClipe };

// ---- efeitos novos ----

const base = () => ({ id: novoId(), ligado: true, nome: "", ativo: null });

/** Faixa analisada = os quadros do trecho agora: do primeiro ao ÚLTIMO (inclusive). */
function analiseDoTrecho(c: ClipeQualquer, tab: TabelaQuadros | null): EfEstabilizar["analise"] | null {
  if (c.tipo !== "trecho" || !tab) return null;
  try {
    const [a, b] = faixaDoTrecho(tab, c.inicio_s, c.inicio_pts, c.fim_s, c.fim_pts);
    const pts = ptsDaTabela(tab);
    const s = (j: number) => ((pts[j] as number) * tab.tb.num) / tab.tb.den;
    return { de_s: s(a), ate_s: s(b - 1), de_pts: pts[a] as number, ate_pts: pts[b - 1] as number };
  } catch {
    return null;
  }
}

/** O efeito com os padrões; null = não dá para criar agora (sem tabela, sem perfil de lente salvo no vídeo). */
async function novo(tipo: Tipo, c: ClipeQualquer, tab: TabelaQuadros | null, ws: string | null, sha: string | null): Promise<Realce | null> {
  switch (tipo) {
    case "desentrelacar":
      return { ...base(), tipo, modo: "quadro", ordem: "auto" };
    case "estabilizar": {
      const analise = analiseDoTrecho(c, tab);
      return analise && { ...base(), tipo, suavizar: SUAVIZAR.novo, borda: "preta", analise, zoom_pct: 0, trf_sha256: null };
    }
    case "lente": {
      // o perfil é o do vídeo, salvo no Realce da análise
      const p = ws && sha ? await Promise.resolve().then(() => commands.realceLenteObter(ws, sha)).catch(() => null) : null;
      return p && { ...base(), tipo, k1: p.k1, k2: p.k2, cx: p.cx, cy: p.cy, perfil: p.nome };
    }
    case "ajustes":
      return { ...base(), tipo, brilho: 0, contraste: 1, gama: 1, saturacao: 1 };
    case "nitidez":
      return { ...base(), tipo, quantidade: 0.5 };
    case "media_temporal":
      return { ...base(), tipo, quadros: MEDIA.novo };
  }
}

/** Há perfil de lente salvo no vídeo? undefined = lendo. O perfil nasce na análise, com a Edição montada:
 *  `reler` confere de novo. */
function useTemLente(ws: string | null, sha: string | null): { tem: boolean | undefined; reler: () => void } {
  const [tem, setTem] = useState<boolean | undefined>(undefined);
  const [vez, setVez] = useState(0);
  useEffect(() => {
    if (!ws || !sha) {
      setTem(false);
      return;
    }
    let vivo = true;
    const guardar = (v: boolean) => void (vivo && setTem(v));
    void Promise.resolve()
      .then(() => commands.realceLenteObter(ws, sha))
      .then((p) => guardar(!!p), () => guardar(false));
    return () => {
      vivo = false;
    };
  }, [ws, sha, vez]);
  return { tem, reler: () => setVez((v) => v + 1) };
}

/** Estabilizar e lente mudam o espaço das marcações do clipe: confirmação com a contagem das que ficam inválidas (§3.2). */
async function confirmado(tipo: string, c: ClipeQualquer, titulo: string, acao: string): Promise<boolean> {
  const doc = useEdicaoStore.getState().doc;
  const n = MUDA_ESPACO.has(tipo) && doc ? marcacoesNoEspaco(doc, c) : 0;
  return n === 0 || askConfirm({ title: titulo, message: n === 1 ? "1 marcação" : `${n} marcações`, confirmLabel: acao, danger: true });
}

// ---- tela ----

export function RealceSecao({ clipe: c, tabela }: SecaoProps<ClipeQualquer>) {
  const ws = useEdicaoStore((s) => s.ws);
  const sha = useShaDe(c.tipo === "trecho" || c.tipo === "congelar" ? c.midia : null);
  const invalidos = useInvalidos();
  const origem = useOrigem(c);
  const lente = useTemLente(ws, sha);
  // do clipe: os de origem conhecidos, e os que o Rust acusou (`item_invalido`), apagados
  const doClipe = c.efeitos.filter(ehRealce);
  // só quem lê quadros da fonte passa pela cadeia de origem; o congelado não tem o que estabilizar
  const oferecidos: Tipo[] = c.tipo === "trecho" ? TIPOS.map((t) => t.tipo) : c.tipo === "congelar" ? TIPOS.map((t) => t.tipo).filter((t) => t !== "estabilizar") : [];
  const linhas = TIPOS.filter((t) => oferecidos.includes(t.tipo) || doClipe.some((e) => e.tipo === t.tipo));
  if (linhas.length === 0) return null;
  const ligar = async (t: (typeof TIPOS)[number]) => {
    if (!(await confirmado(t.tipo, c, t.rotulo, t.rotulo))) return;
    const e = await novo(t.tipo, c, tabela, ws, sha);
    if (e) mudar(t.rotulo, noClipe(c.id, (x) => void (x.efeitos as Draft<Efeito>[]).push(e as Draft<Efeito>)));
    else if (t.tipo === "lente") lente.reler();
  };
  // sem a tabela não há faixa a analisar; sem perfil salvo no vídeo não há lente a aplicar
  const semComo = (tipo: Tipo) => (tipo === "estabilizar" && !analiseDoTrecho(c, tabela)) || (tipo === "lente" && lente.tem === false);
  return (
    <Secao titulo="Realce" acoes={<Estado origem={origem} />}>
      <ul className={styles.efeitos}>
        {linhas.map((t) => {
          const existentes = doClipe.filter((e) => e.tipo === t.tipo);
          if (existentes.length === 0) {
            const Ic = t.icone;
            return (
              <li key={t.tipo} className={styles.efeito} data-realce={t.tipo} onPointerEnter={t.tipo === "lente" && lente.tem === false ? lente.reler : undefined}>
                <div className={styles.efeitoTopo}>
                  <span className={styles.efeitoNome}>
                    <span className={styles.semSeta} aria-hidden />
                    <Ic size={13} aria-hidden />
                    <span>{t.rotulo}</span>
                  </span>
                  <span className={styles.acoes}>
                    <Icone rotulo={t.rotulo} aria-pressed={false} disabled={semComo(t.tipo)} onClick={() => void ligar(t)}>
                      <EyeOff size={13} />
                    </Icone>
                  </span>
                </div>
              </li>
            );
          }
          return existentes.map((e) => <Linha key={e.id} e={e} clipe={c} tabela={tabela} invalido={invalidos.has(e.id)} rotulo={t.rotulo} icone={t.icone} />);
        })}
      </ul>
    </Secao>
  );
}

/** Intermediário em preparo ou que falhou: ícone, sem texto (§8.6). */
function Estado({ origem }: { origem: OrigemDoClipe | undefined }) {
  if (origem?.tipo === "fazendo") return <LoaderCircle size={13} className={styles.dim} aria-hidden data-origem="fazendo" />;
  if (origem?.tipo === "falhou") return <CircleAlert size={13} className={styles.dim} aria-hidden data-origem="falhou" />;
  return origem?.tipo === "pronto" ? <span data-origem="pronto" hidden /> : null;
}

function Linha({ e, clipe, tabela, invalido, rotulo, icone: Ic }: { e: Realce; clipe: ClipeQualquer; tabela: TabelaQuadros | null; invalido: boolean; rotulo: string; icone: LucideIcon }) {
  if (invalido) {
    // forma que o Rust recusa: só o tipo, sem editor
    const { d } = descritor(e, new Set([e.id]));
    const Apagado = d.icone;
    return (
      <li className={`${styles.efeito} ${styles.efeitoApagado}`} data-efeito={e.tipo}>
        <div className={styles.efeitoTopo}>
          <span className={styles.efeitoNome}>
            <span className={styles.semSeta} aria-hidden />
            <Apagado size={13} aria-hidden />
            <code>{d.rotulo}</code>
          </span>
          <span className={styles.acoes}>
            <Icone rotulo="Remover" onClick={() => removerEfeito(e.id, "Remover")}>
              <Trash2 size={13} />
            </Icone>
          </span>
        </div>
      </li>
    );
  }
  return (
    <li className={`${styles.efeito} ${e.ligado ? styles.efeitoAberto : ""}`} data-efeito={e.tipo}>
      <div className={styles.efeitoTopo}>
        <span className={styles.efeitoNome}>
          <span className={styles.semSeta} aria-hidden />
          <Ic size={13} aria-hidden />
          <span>{rotulo}</span>
        </span>
        <span className={styles.acoes}>
          <Icone rotulo={rotulo} aria-pressed={e.ligado} onClick={() => void confirmado(e.tipo, clipe, rotulo, rotulo).then((ok) => ok && ligarEfeito(e.id, !e.ligado, rotulo))}>
            {e.ligado ? <Eye size={13} /> : <EyeOff size={13} />}
          </Icone>
          <Icone rotulo="Remover" onClick={() => void confirmado(e.tipo, clipe, rotulo, "Remover").then((ok) => ok && removerEfeito(e.id, "Remover"))}>
            <Trash2 size={13} />
          </Icone>
        </span>
      </div>
      {e.ligado && e.tipo !== "lente" && (
        <div className={styles.efeitoCorpo}>
          <div className={styles.grade}>
            <Campos e={e} clipe={clipe} tabela={tabela} />
          </div>
        </div>
      )}
    </li>
  );
}

type Mut = (d: Draft<ProjetoEdicao>) => void;
const em = <E extends Realce>(e: E, f: (x: Draft<E>) => void): Mut => noEfeito<E>(e.id, (x) => void (x.tipo === e.tipo && f(x)));
const inteiro = (v: number) => String(Math.round(v));

function Campos({ e, clipe, tabela }: { e: Realce; clipe: ClipeQualquer; tabela: TabelaQuadros | null }) {
  switch (e.tipo) {
    case "desentrelacar": {
      // base de um tique por quadro (AVI, fluxo cru): o segundo campo não tem instante próprio e o motor recusa
      const semCampo = !!tabela && !campoCabe(tabela) && e.modo !== "campo";
      return (
        <Escolha
          rotulo="Desentrelaçar"
          semRotulo
          larga
          valor={e.modo}
          opcoes={semCampo ? [{ v: "quadro", r: "Quadro" }] : [{ v: "quadro", r: "Quadro" }, { v: "campo", r: "Campo" }]}
          aplicar={(v) => em(e, (x) => void (x.modo = v))}
        />
      );
    }
    case "estabilizar": {
      const analise = analiseDoTrecho(clipe, tabela);
      const refazer = async () => {
        if (!analise || !(await confirmado(e.tipo, clipe, "Refazer estabilização", "Refazer estabilização"))) return;
        // análise nova na faixa do clipe de agora; o `.trf` e o zoom voltam pelo intermediário
        pedirDeNovo(e.id);
        mudar("Refazer estabilização", em(e, (x) => void Object.assign(x, { analise, trf_sha256: null, zoom_pct: 0 })));
      };
      return (
        <>
          <Numero rotulo="Suavizar" item={e.id} valor={e.suavizar} min={SUAVIZAR.min} max={SUAVIZAR.max} passo={1} casas={0} aplicar={(v) => em(e, (x) => void (x.suavizar = Math.round(v)))} />
          <Escolha
            rotulo="Borda"
            valor={e.borda}
            opcoes={[
              { v: "preta", r: "Preta" },
              { v: "ampliar", r: "Ampliar" },
            ]}
            // o zoom do "ampliar" é calculado de novo pelo intermediário
            aplicar={(v) => {
              pedirDeNovo(e.id);
              return em(e, (x) => void Object.assign(x, { borda: v, zoom_pct: 0 }));
            }}
          />
          <div className={styles.larga}>
            <Mini data-acao="refazer-estabilizacao" disabled={!analise} onClick={() => void refazer()}>
              <RotateCw size={12} /> Refazer estabilização
            </Mini>
          </div>
        </>
      );
    }
    case "ajustes":
      return (
        <>
          <Deslizante rotulo="Brilho" semRotulo larga item={e.id} valor={e.brilho} min={-0.5} max={0.5} passo={0.01} aplicar={(v) => em(e, (x) => void (x.brilho = v))} />
          <Deslizante rotulo="Contraste" larga item={e.id} valor={e.contraste} min={0} max={2} passo={0.01} aplicar={(v) => em(e, (x) => void (x.contraste = v))} />
          <Deslizante rotulo="Gama" larga item={e.id} valor={e.gama} min={0.1} max={3} passo={0.01} aplicar={(v) => em(e, (x) => void (x.gama = v))} />
          <Deslizante rotulo="Saturação" larga item={e.id} valor={e.saturacao} min={0} max={3} passo={0.01} aplicar={(v) => em(e, (x) => void (x.saturacao = v))} />
        </>
      );
    case "nitidez":
      return <Deslizante rotulo="Nitidez" semRotulo larga item={e.id} valor={e.quantidade} min={0} max={2} passo={0.05} aplicar={(v) => em(e, (x) => void (x.quantidade = v))} />;
    case "media_temporal":
      return (
        <Deslizante rotulo="Média de quadros" semRotulo larga item={e.id} valor={e.quadros} min={MEDIA.min} max={MEDIA.max} passo={1} mostrar={inteiro} aplicar={(v) => em(e, (x) => void (x.quadros = Math.round(v)))} />
      );
    case "lente":
      return null;
  }
}
