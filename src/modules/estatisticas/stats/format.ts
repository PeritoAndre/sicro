/** Formatação pt-BR pura (sem dependências), usada por gráficos e exportação. */

export function nf(n: number, decimals = 0): string {
  if (!Number.isFinite(n)) return "—";
  return n.toLocaleString("pt-BR", {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
}

export function fmtBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = bytes;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const unit = units[i] ?? "B";
  return `${v.toLocaleString("pt-BR", { maximumFractionDigits: i === 0 ? 0 : 1 })} ${unit}`;
}

export function pct(part: number, total: number): number {
  if (total <= 0) return 0;
  return (part / total) * 100;
}

export function fmtPct(value: number, decimals = 0): string {
  return `${nf(value, decimals)}%`;
}

export function fmtDurationS(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0s";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${m}min`;
  if (m > 0) return `${m}min ${sec}s`;
  return `${sec}s`;
}

