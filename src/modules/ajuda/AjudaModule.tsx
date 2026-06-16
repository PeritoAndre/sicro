/**
 * AjudaModule — Manual do SICRO dentro do app.
 *
 * Renderiza `docs/MANUAL_SICRO.md` (fonte ÚNICA — editar o .md atualiza a ajuda)
 * com `marked`, monta um índice navegável a partir dos títulos e intercepta os
 * links âncora internos (#secao) pra rolar sem confundir o HashRouter.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { marked } from "marked";
import { HelpCircle, Search } from "lucide-react";
// Fonte única do manual (bundlada como texto). Editar o .md → atualiza a Ajuda.
import manualMd from "../../../docs/MANUAL_SICRO.md?raw";
import styles from "./AjudaModule.module.css";

interface TocItem {
  id: string;
  label: string;
  level: number;
}

/**
 * Slug compatível com o GitHub (minúsculas, sem pontuação, espaços→hífen,
 * MANTÉM acentos) — pra os links do "Sumário" do próprio manual baterem com os
 * ids dos títulos. Desambigua duplicados com sufixo numérico.
 */
function slugify(text: string, used: Set<string>): string {
  const base =
    text
      .trim()
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\s-]/gu, "")
      .replace(/\s+/g, "-") || "secao";
  let slug = base;
  let i = 1;
  while (used.has(slug)) slug = `${base}-${i++}`;
  used.add(slug);
  return slug;
}

export function AjudaModule() {
  const html = useMemo(() => marked.parse(manualMd) as string, []);
  const articleRef = useRef<HTMLDivElement>(null);
  const [toc, setToc] = useState<TocItem[]>([]);
  const [query, setQuery] = useState("");

  // Após renderizar o HTML, dá id a cada título e monta o índice (h2/h3).
  useEffect(() => {
    const root = articleRef.current;
    if (!root) return;
    const used = new Set<string>();
    const items: TocItem[] = [];
    // Dá id a TODOS os títulos (pros links âncora do manual funcionarem), mas o
    // índice lateral mostra só as SEÇÕES principais (h2) — listar h3 também
    // deixava a lista densa e ilegível.
    root.querySelectorAll("h1, h2, h3").forEach((el) => {
      const text = el.textContent ?? "";
      const id = slugify(text, used);
      el.id = id;
      const level = Number(el.tagName.slice(1));
      if (level === 2) items.push({ id, label: text, level });
    });
    setToc(items);
  }, [html]);

  const goTo = (id: string) => {
    const el = articleRef.current?.querySelector(`#${CSS.escape(id)}`);
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  // Links âncora internos (#...) do próprio manual: rola na mão (evita que o
  // HashRouter interprete o # como rota). Links externos seguem normais.
  const onContentClick = (e: React.MouseEvent) => {
    const anchor = (e.target as HTMLElement).closest("a");
    const href = anchor?.getAttribute("href") ?? "";
    if (!anchor || !href.startsWith("#")) return;
    e.preventDefault();
    goTo(decodeURIComponent(href.slice(1)));
  };

  const filteredToc = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return toc;
    return toc.filter((t) => t.label.toLowerCase().includes(q));
  }, [toc, query]);

  return (
    <div className={styles.wrap}>
      <aside className={styles.toc}>
        <div className={styles.tocHead}>
          <HelpCircle size={16} aria-hidden />
          <span>Manual do SICRO</span>
        </div>
        <label className={styles.search}>
          <Search size={13} aria-hidden />
          <input
            type="search"
            placeholder="Buscar no índice…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label="Buscar no índice do manual"
          />
        </label>
        <nav className={styles.tocList} aria-label="Índice do manual">
          {filteredToc.map((t) => (
            <button
              key={t.id}
              type="button"
              className={styles.tocItem}
              onClick={() => goTo(t.id)}
              title={t.label}
            >
              {t.label}
            </button>
          ))}
          {filteredToc.length === 0 && (
            <p className={styles.tocEmpty}>Nada encontrado no índice.</p>
          )}
        </nav>
      </aside>

      <article
        ref={articleRef}
        className={styles.content}
        onClick={onContentClick}
        // Conteúdo é o nosso próprio manual (estático/confiável) — não é entrada
        // de usuário, então o innerHTML é seguro aqui.
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  );
}
