/**
 * Avisos técnicos do arquivo em português de gente: o que significa para o cálculo.
 * O texto cru do ffprobe fica só nos detalhes de perito.
 */

import styles from "./Medir.module.css";

function explain(w: string): { title: string; text: string } {
  if (/vari[aá]vel|VFR/i.test(w)) {
    return {
      title: "Vídeo com ritmo irregular de quadros",
      text: "Alguns quadros chegam fora do compasso. O SICRO usa o horário real de cada quadro coletado, então tempos e velocidades continuam valendo; o número do quadro é aproximado.",
    };
  }
  if (/seek|delta|solicitad/i.test(w)) {
    return {
      title: "Quadro um pouco diferente do pedido",
      text: "O quadro extraído ficou a alguns milissegundos do instante pedido. O cálculo usa o horário real dele.",
    };
  }
  return { title: "Aviso técnico do arquivo", text: w };
}

export function VideoHealth({ warnings, perito }: { warnings: string[]; perito: boolean }) {
  if (warnings.length === 0) return null;
  const seen = new Set<string>();
  const items = warnings
    .map((w) => ({ raw: w, ...explain(w) }))
    .filter((it) => (seen.has(it.title) ? false : (seen.add(it.title), true)));
  return (
    <>
      {items.map((it) => (
        <div key={it.title} className={styles.health} role="note">
          <b>{it.title}</b>
          <span>{it.text}</span>
          {perito && <code>{it.raw}</code>}
        </div>
      ))}
    </>
  );
}
