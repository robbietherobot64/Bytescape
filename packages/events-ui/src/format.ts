// Formatting contracts shared by card, detail, and panel (3+ call sites each;
// the mappings encode token/magic constants that should not be re-derived).

/** Date line for cards/panels: "Happened on …" for past, date · time ranges
 * for upcoming — the shape the old events UI used, kept stable. */
export function formatEventDate(
  startIso: string,
  endIso?: string | null,
  isUpcoming?: boolean,
): string {
  const start = new Date(startIso);
  const end = endIso ? new Date(endIso) : undefined;

  const fmtDate = (d: Date) =>
    d.toLocaleDateString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  const fmtTime = (d: Date) =>
    d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });

  if (isUpcoming === false) {
    return `Happened on ${fmtDate(start)}`;
  }
  if (end && start.toDateString() === end.toDateString()) {
    return `${fmtDate(start)} · ${fmtTime(start)} – ${fmtTime(end)}`;
  }
  if (end) {
    return `${fmtDate(start)} – ${fmtDate(end)}`;
  }
  return `${fmtDate(start)} · ${fmtTime(start)}`;
}

/** Lexicon status token → display label + tone (default = scheduled). */
export function eventStatusLabel(status: string): { text: string; tone: "normal" | "warn" | "bad" } {
  if (status.endsWith("#cancelled")) return { text: "CANCELLED", tone: "bad" };
  if (status.endsWith("#postponed")) return { text: "POSTPONED", tone: "warn" };
  if (status.endsWith("#rescheduled")) return { text: "RESCHEDULED", tone: "warn" };
  if (status.endsWith("#planned")) return { text: "PLANNED", tone: "normal" };
  return { text: "SCHEDULED", tone: "normal" };
}
