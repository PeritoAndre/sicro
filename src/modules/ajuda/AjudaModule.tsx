/** Manual do SICRO dentro do app: renderiza `docs/MANUAL_SICRO.md` com `marked` + índice lateral. */

import { useEffect, useMemo, useRef, useState } from "react";
import { marked } from "marked";
import { HelpCircle, Search } from "lucide-react";
// Fonte única: editar o .md atualiza a Ajuda.
import manualMd from "../../../docs/MANUAL_SICRO.md?raw";
import { FeedbackButton } from "./FeedbackButton";
import styles from "./AjudaModule.module.css";

interface TocItem {
  id: string;
  label: string;
  level: number;
}

/** Slug no padrão do GitHub (mantém acentos) para os links do Sumário do manual baterem. */
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

  useEffect(() => {
    const root = articleRef.current;
    if (!root) return;
    const used = new Set<string>();
    const items: TocItem[] = [];
    // Id em todos os títulos (âncoras do manual); o índice lateral só lista h2.
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

  // Âncoras internas rolam na mão: o HashRouter interpretaria o # como rota.
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
        <div className={styles.tocFoot}>
          <FeedbackButton />
        </div>
      </aside>

      <article
        ref={articleRef}
        className={styles.content}
        onClick={onContentClick}
        // Manual próprio, estático — não é entrada de usuário.
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  );
}
