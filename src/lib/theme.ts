import { writable } from "svelte/store";
import { getTheme, setTheme } from "./commands";

export type Theme = "light" | "dark";

const isValidTheme = (value: string | null): value is Theme =>
  value === "light" || value === "dark";

export const theme = writable<Theme>("light");

if (typeof window !== "undefined") {
  (async () => {
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    let configTheme: string | null = null;

    try {
      configTheme = await getTheme();
    } catch (err) {
      console.warn("Could not get theme:", err);
    }

    const resolved: Theme = isValidTheme(configTheme)
      ? configTheme
      : prefersDark
        ? "dark"
        : "light";

    theme.set(resolved);

     theme.subscribe(async (v) => {
      document.documentElement.classList.toggle("dark", v === "dark");
      await setTheme(v);
    });
  })();
}
