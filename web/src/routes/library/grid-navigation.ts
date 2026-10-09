// Arrow-key movement between library items (grid or list).

export type NavKey = "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" | "Home" | "End";

export function isNavKey(key: string): key is NavKey {
  return ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(key);
}

/** The index focus moves to, or `null` to stay put. */
export function moveIndex(
  key: NavKey,
  index: number,
  count: number,
  columns: number,
): number | null {
  if (count === 0) return null;
  const cols = Math.max(1, columns);
  let next: number;
  switch (key) {
    case "ArrowLeft":
      next = index - 1;
      break;
    case "ArrowRight":
      next = index + 1;
      break;
    case "ArrowUp":
      next = index - cols;
      break;
    case "ArrowDown":
      // From a short last row's neighbour, land on the last item.
      next =
        index + cols < count
          ? index + cols
          : Math.floor(index / cols) < Math.floor((count - 1) / cols)
            ? count - 1
            : index;
      break;
    case "Home":
      next = 0;
      break;
    case "End":
      next = count - 1;
      break;
  }
  if (next < 0 || next >= count || next === index) return null;
  return next;
}

/** Columns in a CSS grid: items sharing the first item's top edge. */
export function columnsOf(items: readonly HTMLElement[]): number {
  const first = items[0];
  if (!first) return 1;
  const top = first.offsetTop;
  let cols = 0;
  for (const item of items) {
    if (item.offsetTop !== top) break;
    cols += 1;
  }
  return Math.max(1, cols);
}
