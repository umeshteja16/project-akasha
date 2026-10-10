import { describe, expect, it } from "vitest";
import type { ActivityItem } from "@/api/activity";
import { dayLabel, groupByDay } from "./group";

function item(id: string, created_at: string): ActivityItem {
  return {
    id,
    kind: "file.uploaded",
    category: "files",
    created_at,
    details: {},
  } as ActivityItem;
}

describe("groupByDay", () => {
  const now = new Date(2026, 9, 10, 15, 0);
  it("labels today and yesterday and keeps order", () => {
    const items = [
      item("a", new Date(2026, 9, 10, 9, 30).toISOString()),
      item("b", new Date(2026, 9, 10, 8, 0).toISOString()),
      item("c", new Date(2026, 9, 9, 23, 0).toISOString()),
      item("d", new Date(2026, 9, 1, 12, 0).toISOString()),
    ];
    const groups = groupByDay(items, now);
    expect(groups.map((g) => g.label.slice(0, 9))).toEqual([
      "Today",
      "Yesterday",
      dayLabel(new Date(2026, 9, 1), now).slice(0, 9),
    ]);
    expect(groups[0]?.items.map((i) => i.id)).toEqual(["a", "b"]);
    expect(dayLabel(new Date(2025, 0, 2), now)).toMatch(/2025/);
  });
});
