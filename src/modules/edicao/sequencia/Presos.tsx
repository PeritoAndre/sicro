/** Presos ao clipe (§8.2, P1.7): o que tem `pos.ref = "clipe"` (título, terço inferior, imagem, narração), com
 *  "Adicionar" → "Título", "Terço inferior", "Imagem", "Narração". Título e terço inferior nascem no modelo (evento e
 *  terço inferior, sem fundo; o terço vai até o fim do clipe) e ficam selecionados para o texto. Lê tudo da store. */

import { useEffect, useRef, useState } from "react";
import { Image as IconeImagem, Mic, Music, Plus, Type, Video, Volume2 } from "lucide-react";
import { open as abrirArquivo } from "@tauri-apps/plugin-dialog";
import { commands } from "@core/commands";
import type { Id, ItemSel, ProjetoEdicao, SobTitulo, TrilhaSobreposicao } from "@domain/edicao";
import { pushToast } from "@components/toast/toastStore";
import { separarErro, useEdicaoStore } from "../store/edicaoStore";
import { garantirMidia, novoId } from "../modelo/projeto";
import { tituloNoModelo } from "../modelo/titulos";
import { useMenuAberto } from "../menus";
import { DURACAO_PADRAO, presosAoClipe, type Preso } from "./contas";
import { prenderImagem, prenderNarracao } from "./novos";
// as listas que o Rust aceita (`armazem::{EXT_IMAGEM, EXT_AUDIO}`)
import { EXTENSOES } from "../inspetor/secoes/importar";
import styles from "./Sequencia.module.css";

export const PRESOS_PRONTOS = true;

export type TipoPreso = "Título" | "Terço inferior" | "Imagem" | "Narração";
const TIPOS: readonly TipoPreso[] = ["Título", "Terço inferior", "Imagem", "Narração"];

const ICONE = { titulo: Type, imagem: IconeImagem, video: Video, narracao: Mic, musica: Music, efeitos: Volume2 } as const;
/** Nome do item preso sem rótulo próprio (só o ícone aparece): o do tipo. */
const NOME = { titulo: "Título", imagem: "Imagem", video: "Vídeo", narracao: "Narração", musica: "Música", efeitos: "Efeitos" } as const;

const NEUTRA = { x: 0.5, y: 0.5, escala: 1, rotacao: 0, opacidade: 1 };

/** Título preso ao clipe, no modelo, na trilha de sobreposição de baixo (criada se não houver). "Título" = evento por
 *  `DURACAO_PADRAO`; "Terço inferior" até o fim do clipe (`duracao_s` nula). Devolve o item. */
export function prenderTituloNoModelo(d: ProjetoEdicao, clipe: Id, modelo: "evento" | "terco_inferior"): ItemSel {
  if (!d.sobreposicoes[0]) d.sobreposicoes.push({ id: novoId(), nome: "", oculta: false, clipes: [] });
  const t = d.sobreposicoes[0] as TrilhaSobreposicao;
  const c: SobTitulo = {
    tipo: "titulo",
    id: novoId(),
    rotulo: "",
    pos: { ref: "clipe", clipe, desloc: 0 },
    duracao_s: modelo === "terco_inferior" ? null : DURACAO_PADRAO,
    transformacao: { ...NEUTRA },
    entrada_s: 0,
    saida_s: 0,
    efeitos: [],
    titulo: tituloNoModelo(modelo, true, novoId(), { w: d.saida.largura, h: d.saida.altura }),
  };
  t.clipes.push(c);
  return { k: "livre", id: c.id, trilha: t.id };
}

const st = () => useEdicaoStore.getState();

/** Importa um arquivo para o projeto (a pasta do projeto precisa existir antes da mídia própria). */
async function importar(tipo: "imagem" | "audio"): Promise<{ id: Id; nome: string } | null> {
  const s = st();
  if (!s.doc || !s.ws) return null;
  const escolhido = await abrirArquivo({ multiple: false, filters: [{ name: tipo === "imagem" ? "Imagem" : "Narração", extensions: EXTENSOES[tipo] }] });
  if (typeof escolhido !== "string") return null;
  await s.descarregar().catch(() => {});
  const ref = await commands.edicaoMidiaImportar(s.ws, s.doc.id, escolhido, tipo);
  let id = ref.id;
  st().alterar(tipo === "imagem" ? "Imagem" : "Narração", (d) => {
    id = garantirMidia(d, ref);
  });
  return { id, nome: ref.nome };
}

/** Falha de importação: a frase do catálogo, no aviso do app. */
const falhou = (e: unknown) => {
  const r = separarErro(e);
  pushToast("error", r.frase || r.codigo || "");
  return null;
};

export async function adicionarPreso(clipe: Id, t: TipoPreso): Promise<void> {
  if (t === "Título" || t === "Terço inferior") {
    let item: ItemSel | null = null;
    st().alterar(t, (d) => {
      item = prenderTituloNoModelo(d, clipe, t === "Título" ? "evento" : "terco_inferior");
    });
    if (item) st().selecionar({ itens: [item], foco: item, efeito: null });
    return;
  }
  const m = await importar(t === "Imagem" ? "imagem" : "audio").catch(falhou);
  if (!m) return;
  st().alterar(t, (d) => {
    if (t === "Imagem") prenderImagem(d, clipe, m.id);
    else prenderNarracao(d, clipe, m.id, m.nome.replace(/\.[^.]+$/, ""));
  });
}

function selecionarPreso(p: Preso) {
  const item: ItemSel = { k: p.k, id: p.id, trilha: p.trilha };
  st().selecionar({ itens: [item], foco: item, efeito: null });
}

export function Presos({ clipe }: { clipe: Id }) {
  const doc = useEdicaoStore((s) => s.doc);
  const soLeitura = useEdicaoStore((s) => s.soLeitura);
  // o cartão em foco (o mesmo da Sequência): seleção do clipe, senão o clipe do cursor
  const emFoco = useEdicaoStore((s) => (s.selecao.foco?.k === "clipe" ? s.selecao.foco.id : s.cursor.modo === "clipe" ? s.cursor.clipe : null) === clipe);
  const [menu, setMenu] = useState(false);
  const menuRef = useRef<HTMLSpanElement | null>(null);
  useMenuAberto(menu);
  // menu aberto: clique fora ou Esc fecha
  useEffect(() => {
    if (!menu) return;
    const fora = (e: Event) => {
      if (!menuRef.current?.contains(e.target as Node)) setMenu(false);
    };
    const tecla = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenu(false);
    };
    window.addEventListener("pointerdown", fora, true);
    window.addEventListener("keydown", tecla);
    return () => {
      window.removeEventListener("pointerdown", fora, true);
      window.removeEventListener("keydown", tecla);
    };
  }, [menu]);
  if (!doc) return null;
  const presos = presosAoClipe(doc, clipe);
  if (presos.length === 0 && !emFoco) return null;
  const parar = (e: { stopPropagation(): void }) => e.stopPropagation();
  return (
    <div className={styles.presos} onClick={parar} data-presos={clipe}>
      {presos.map((x) => {
        const I = ICONE[x.tipo];
        return (
          <button type="button" key={x.id} className={styles.preso} onClick={() => selecionarPreso(x)} title={x.rotulo || NOME[x.tipo]} aria-label={x.rotulo || NOME[x.tipo]} data-preso={x.id}>
            <I size={12} aria-hidden />
            {x.rotulo && <span>{x.rotulo}</span>}
          </button>
        );
      })}
      {!soLeitura && (
        <span className={styles.adicionar} ref={menuRef}>
          <button type="button" className={styles.mini} onClick={() => setMenu((m) => !m)} aria-expanded={menu}>
            <Plus size={12} aria-hidden />
            Adicionar
          </button>
          {menu && (
            <span className={styles.menu} role="menu">
              {TIPOS.map((t) => (
                <button
                  type="button"
                  role="menuitem"
                  key={t}
                  onClick={() => {
                    setMenu(false);
                    void adicionarPreso(clipe, t);
                  }}
                >
                  {t}
                </button>
              ))}
            </span>
          )}
        </span>
      )}
    </div>
  );
}
