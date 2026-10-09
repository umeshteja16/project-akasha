// Human-friendly numbers and dates.

const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** 1536 → "1.5 KB" (binary multiples, the way file managers show sizes). */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits).replace(/\.0$/, "")} ${UNITS[unit]}`;
}

const relative = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
const STEPS: ReadonlyArray<[Intl.RelativeTimeFormatUnit, number]> = [
  ["second", 60],
  ["minute", 60],
  ["hour", 24],
  ["day", 7],
  ["week", 4.35],
  ["month", 12],
  ["year", Number.POSITIVE_INFINITY],
];

/** "just now", "5 minutes ago", "yesterday", "3 weeks ago". */
export function formatRelative(iso: string, now: number = Date.now()): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  let delta = (then - now) / 1000;
  if (Math.abs(delta) < 45) return "just now";
  for (const [unit, size] of STEPS) {
    if (Math.abs(delta) < size) return relative.format(Math.round(delta), unit);
    delta /= size;
  }
  return "";
}

const dateTime = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

/** Absolute date and time in the viewer's locale. */
export function formatDateTime(iso: string): string {
  const then = Date.parse(iso);
  return Number.isNaN(then) ? "" : dateTime.format(then);
}
