/**
 * FieldSuggestionList — dropdown de autocomplete dos campos automáticos.
 *
 * Renderizado pelo `FieldSuggestion` (plugin @tiptap/suggestion) num portal
 * ancorado no caret. Funciona como o autocomplete de uma IDE:
 *   - filtra conforme o perito digita depois do `{`;
 *   - ↑/↓ navega, Enter/Tab confirma, Esc fecha (tratado via ref.onKeyDown);
 *   - clique seleciona.
 *
 * Expõe `onKeyDown` via ref para o plugin encaminhar as teclas (o foco
 * permanece no editor — o dropdown não captura foco).
 */

import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type { LaudoFieldDefinition } from "./catalog";

export interface FieldSuggestionListRef {
  onKeyDown: (event: KeyboardEvent) => boolean;
}

interface Props {
  items: LaudoFieldDefinition[];
  command: (item: LaudoFieldDefinition) => void;
}

const GROUP_LABEL: Record<string, string> = {
  identificacao: "Identificação",
  local: "Local / data",
  partes: "Partes",
  veiculos: "Veículos",
  vestigios: "Vestígios",
  midia: "Mídia",
  sistema: "Sistema",
};

export const FieldSuggestionList = forwardRef<FieldSuggestionListRef, Props>(
  function FieldSuggestionList({ items, command }, ref) {
    const [selected, setSelected] = useState(0);
    const listRef = useRef<HTMLDivElement | null>(null);

    // Reseta a seleção quando a lista muda (o usuário digitou mais).
    useEffect(() => {
      setSelected(0);
    }, [items]);

    // Mantém o item ativo visível ao navegar com as setas.
    useLayoutEffect(() => {
      const el = listRef.current?.querySelector<HTMLElement>(
        `[data-idx="${selected}"]`,
      );
      el?.scrollIntoView({ block: "nearest" });
    }, [selected]);

    useImperativeHandle(ref, () => ({
      onKeyDown: (event: KeyboardEvent) => {
        if (items.length === 0) return false;
        if (event.key === "ArrowDown") {
          setSelected((i) => (i + 1) % items.length);
          return true;
        }
        if (event.key === "ArrowUp") {
          setSelected((i) => (i - 1 + items.length) % items.length);
          return true;
        }
        if (event.key === "Enter" || event.key === "Tab") {
          const item = items[selected];
          if (item) command(item);
          return true;
        }
        return false;
      },
    }));

    if (items.length === 0) {
      return (
        <div className="sicro-field-suggest" ref={listRef}>
          <div className="sicro-field-suggest-empty">Nenhum campo encontrado</div>
        </div>
      );
    }

    return (
      <div className="sicro-field-suggest" ref={listRef} role="listbox">
        {items.map((item, idx) => (
          <button
            key={item.key}
            type="button"
            role="option"
            aria-selected={idx === selected}
            data-idx={idx}
            className={
              idx === selected
                ? "sicro-field-suggest-item is-active"
                : "sicro-field-suggest-item"
            }
            // mousedown (não click) pra não roubar a seleção do editor antes do
            // comando rodar.
            onMouseDown={(e) => {
              e.preventDefault();
              command(item);
            }}
            onMouseEnter={() => setSelected(idx)}
          >
            <span className="sicro-field-suggest-label">{item.label}</span>
            <span className="sicro-field-suggest-key">{`{${item.key}}`}</span>
            <span className="sicro-field-suggest-group">
              {GROUP_LABEL[item.group] ?? item.group}
            </span>
          </button>
        ))}
      </div>
    );
  },
);
