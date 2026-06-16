/**
 * useMathHydration (Rodada 2) — gera o PNG das fórmulas IMPORTADAS.
 *
 * A importação OMML (Rust) só consegue produzir o `latex` de cada fórmula — não
 * roda MathLive pra rasterizar. Então os nós math importados chegam SEM
 * `render_png` (o artefato de exibição usado no editor, HTML e DOCX).
 *
 * Este hook, ao carregar o laudo, varre o doc em busca de nós math com `latex`
 * mas sem `render_png`, renderiza cada um (MathLive → PNG, lazy) e grava os
 * attrs de volta no nó. Depois disso a fórmula importada exibe e exporta igual
 * às criadas no SICRO. O write é `addToHistory:false` (não polui o undo) mas
 * suja o doc → o autosave persiste o PNG (na próxima abertura já está pronto).
 *
 * Idempotente: roda no load e a cada update, mas só age quando há fórmula
 * pendente; após hidratar, não faz mais nada.
 */

import { useEffect, useRef } from "react";
import type { Editor } from "@tiptap/react";
import { MATH_RENDER_VERSION } from "../document-engine/nodes";

const MATH_TYPES = new Set(["mathInline", "mathBlock"]);

export function useMathHydration(editor: Editor | null): void {
  const running = useRef(false);

  useEffect(() => {
    if (!editor) return undefined;

    const collect = (): Array<{ pos: number; latex: string; display: boolean }> => {
      const targets: Array<{ pos: number; latex: string; display: boolean }> = [];
      editor.state.doc.descendants((node, pos) => {
        if (MATH_TYPES.has(node.type.name)) {
          const latex = (node.attrs["latex"] as string | undefined) ?? "";
          const png = node.attrs["render_png"] as string | null | undefined;
          const v = node.attrs["render_v"] as number | null | undefined;
          // Regera quando falta PNG OU quando o PNG é de um engine antigo
          // (MathLive em branco): render_v != versão atual.
          if (latex.trim() && (!png || v !== MATH_RENDER_VERSION)) {
            targets.push({
              pos,
              latex,
              display: node.type.name === "mathBlock",
            });
          }
        }
        return true;
      });
      return targets;
    };

    const run = async () => {
      if (running.current) return;
      const targets = collect();
      if (targets.length === 0) return;
      running.current = true;
      try {
        const { latexToPng } = await import(
          "../document-engine/math/mathRender"
        );
        for (const t of targets) {
          let render: Awaited<ReturnType<typeof latexToPng>>;
          try {
            render = await latexToPng(t.latex, { display: t.display });
          } catch {
            continue; // fórmula inválida — o perito reedita manualmente
          }
          // Posição estável: mudar só attrs não altera o nodeSize. Mesmo assim
          // revalidamos (se o usuário editou no meio-tempo, nodeAt não casa →
          // pula; o próximo ciclo pega na posição nova).
          const node = editor.state.doc.nodeAt(t.pos);
          if (
            !node ||
            !MATH_TYPES.has(node.type.name) ||
            (node.attrs["render_png"] &&
              node.attrs["render_v"] === MATH_RENDER_VERSION)
          ) {
            continue;
          }
          const tr = editor.state.tr.setNodeMarkup(t.pos, undefined, {
            ...node.attrs,
            render_png: render.dataUri,
            render_w_cm: render.widthCm,
            render_h_cm: render.heightCm,
            render_v: MATH_RENDER_VERSION,
          });
          tr.setMeta("addToHistory", false);
          editor.view.dispatch(tr);
        }
      } finally {
        running.current = false;
      }
    };

    let timer: number | undefined;
    const schedule = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => void run(), 150);
    };
    schedule(); // após o conteúdo carregar
    editor.on("update", schedule);
    return () => {
      window.clearTimeout(timer);
      editor.off("update", schedule);
    };
  }, [editor]);
}
