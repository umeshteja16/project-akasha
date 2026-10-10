// Grouping the timeline by calendar day (local time).

import type { ActivityItem } from "@/api/activity";

export interface DayGroup {
  /** `YYYY-MM-DD`, local. */
  key: string;
  label: string;
  items: ActivityItem[];
}

function dayKey(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

const weekday = new Intl.DateTimeFormat(undefined, {
  weekday: "long",
  month: "long",
  day: "numeric",
});
const withYear = new Intl.DateTimeFormat(undefined, {
  weekday: "long",
  month: "long",
  day: "numeric",
  year: "numeric",
});

export function dayLabel(date: Date, now: Date = new Date()): string {
  const today = dayKey(now);
  const yesterday = dayKey(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1));
  const key = dayKey(date);
  if (key === today) return "Today";
  if (key === yesterday) return "Yesterday";
  return date.getFullYear() === now.getFullYear() ? weekday.format(date) : withYear.format(date);
}

/** Items (newest first) into day groups, keeping the order. */
export function groupByDay(items: readonly ActivityItem[], now: Date = new Date()): DayGroup[] {
  const groups: DayGroup[] = [];
  for (const item of items) {
    const date = new Date(item.created_at);
    const key = dayKey(date);
    const last = groups[groups.length - 1];
    if (last?.key === key) last.items.push(item);
    else groups.push({ key, label: dayLabel(date, now), items: [item] });
  }
  return groups;
}

const time = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });

export function timeOf(iso: string): string {
  return time.format(new Date(iso));
}
