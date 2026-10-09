// Runs before first paint: apply the saved theme so the page never flashes.
// Keep in sync with src/lib/theme.tsx (storage key and values).
(() => {
  let pref = "system";
  try {
    pref = localStorage.getItem("akasha-theme") || "system";
  } catch {
    // Storage blocked: follow the system.
  }
  const dark =
    pref === "dark" ||
    (pref !== "light" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  const root = document.documentElement;
  root.dataset.theme = dark ? "dark" : "light";
  root.style.colorScheme = dark ? "dark" : "light";
})();
