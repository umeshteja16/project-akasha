// Opens the keyboard shortcuts sheet from anywhere (`?`, the palette, the user menu).

type Listener = () => void;
const listeners = new Set<Listener>();

export function openShortcuts() {
  for (const listener of listeners) listener();
}

export function onOpenShortcuts(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
