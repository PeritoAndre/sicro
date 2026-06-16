/**
 * String/date formatters used across the UI.
 * Keep these locale-aware (pt-BR) but pure — no side effects.
 */

const PT_BR = "pt-BR";

const dateTimeFmt = new Intl.DateTimeFormat(PT_BR, {
  day: "2-digit",
  month: "2-digit",
  year: "numeric",
  hour: "2-digit",
  minute: "2-digit",
});

const dateFmt = new Intl.DateTimeFormat(PT_BR, {
  day: "2-digit",
  month: "2-digit",
  year: "numeric",
});

const relativeFmt = new Intl.RelativeTimeFormat(PT_BR, { numeric: "auto" });

export function formatDateTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  return dateTimeFmt.format(d);
}

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  return dateFmt.format(d);
}

export function formatRelative(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  const diffMs = d.getTime() - Date.now();
  const diffSec = Math.round(diffMs / 1000);
  const absSec = Math.abs(diffSec);

  if (absSec < 60) return relativeFmt.format(diffSec, "second");
  if (absSec < 3600) return relativeFmt.format(Math.round(diffSec / 60), "minute");
  if (absSec < 86400) return relativeFmt.format(Math.round(diffSec / 3600), "hour");
  if (absSec < 86400 * 30)
    return relativeFmt.format(Math.round(diffSec / 86400), "day");
  if (absSec < 86400 * 365)
    return relativeFmt.format(Math.round(diffSec / (86400 * 30)), "month");
  return relativeFmt.format(Math.round(diffSec / (86400 * 365)), "year");
}
