import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { THEME_STORAGE_KEY, ThemeProvider, useTheme } from "./theme";

function Probe() {
  const { preference, resolved, toggle, setPreference } = useTheme();
  return (
    <div>
      <p>
        {preference}/{resolved}
      </p>
      <button type="button" onClick={toggle}>
        toggle
      </button>
      <button type="button" onClick={() => setPreference("system")}>
        system
      </button>
    </div>
  );
}

describe("ThemeProvider", () => {
  it("follows the system by default", () => {
    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );
    // The jsdom matchMedia stub reports light.
    expect(screen.getByText("system/light")).toBeInTheDocument();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("toggles, applies and persists the choice", async () => {
    const user = userEvent.setup();
    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );
    await user.click(screen.getByRole("button", { name: "toggle" }));
    expect(screen.getByText("dark/dark")).toBeInTheDocument();
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.style.colorScheme).toBe("dark");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");

    await user.click(screen.getByRole("button", { name: "system" }));
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBeNull();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("restores a saved choice", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "dark");
    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );
    expect(screen.getByText("dark/dark")).toBeInTheDocument();
  });
});
