/** Perguntas no tema do SICRO, no lugar de prompt/confirm nativos (que abrem "JavaScript - tauri://…"). */

import { useEffect, useRef, useState } from "react";
import { create } from "zustand";
import { Button } from "@components/Button/Button";
import { Dialog } from "./Dialog";

type Question =
  | {
      kind: "text";
      title: string;
      message?: string;
      value: string;
      confirmLabel: string;
      numeric: boolean;
      resolve: (v: string | null) => void;
    }
  | {
      kind: "confirm";
      title: string;
      message?: string;
      confirmLabel: string;
      danger: boolean;
      resolve: (v: boolean) => void;
    };

const useAsk = create<{ q: Question | null; set: (q: Question | null) => void }>((set) => ({
  q: null,
  set: (q) => set({ q }),
}));

/** Texto digitado, ou null se cancelou. */
export function askText(o: {
  title: string;
  message?: string;
  defaultValue?: string;
  confirmLabel?: string;
  numeric?: boolean;
}): Promise<string | null> {
  return new Promise((resolve) =>
    useAsk.getState().set({
      kind: "text",
      title: o.title,
      message: o.message,
      value: o.defaultValue ?? "",
      confirmLabel: o.confirmLabel ?? "OK",
      numeric: !!o.numeric,
      resolve,
    }),
  );
}

export function askConfirm(o: {
  title: string;
  message?: string;
  confirmLabel?: string;
  danger?: boolean;
}): Promise<boolean> {
  return new Promise((resolve) =>
    useAsk.getState().set({
      kind: "confirm",
      title: o.title,
      message: o.message,
      confirmLabel: o.confirmLabel ?? "Confirmar",
      danger: !!o.danger,
      resolve,
    }),
  );
}

/** Montado uma vez na casca do app. */
export function AskHost() {
  const q = useAsk((s) => s.q);
  const set = useAsk((s) => s.set);
  const [val, setVal] = useState("");
  const input = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (q?.kind !== "text") return;
    setVal(q.value);
    const t = window.setTimeout(() => {
      input.current?.focus();
      input.current?.select();
    }, 0);
    return () => window.clearTimeout(t);
  }, [q]);

  if (!q) return null;
  const close = (ok: boolean) => {
    set(null);
    if (q.kind === "text") q.resolve(ok ? val : null);
    else q.resolve(ok);
  };

  return (
    <Dialog
      open
      title={q.title}
      onClose={() => close(false)}
      footer={
        <>
          <Button variant="secondary" onClick={() => close(false)}>
            Cancelar
          </Button>
          <Button variant={q.kind === "confirm" && q.danger ? "danger" : "primary"} onClick={() => close(true)}>
            {q.confirmLabel}
          </Button>
        </>
      }
    >
      {q.message && <p style={{ margin: "0 0 10px", whiteSpace: "pre-line", lineHeight: 1.45 }}>{q.message}</p>}
      {q.kind === "text" && (
        <input
          ref={input}
          value={val}
          inputMode={q.numeric ? "decimal" : undefined}
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              close(true);
            }
          }}
          style={{
            width: "100%",
            padding: "8px 10px",
            font: "inherit",
            color: "var(--sicro-fg)",
            background: "var(--sicro-surface-2)",
            border: "1px solid var(--sicro-border)",
            borderRadius: "var(--radius-sm)",
          }}
        />
      )}
    </Dialog>
  );
}
