import { createContext, useContext } from "react";

export type ThemePreference = "dark" | "light" | "system";

export interface ThemeContextValue {
  preference: ThemePreference;
  resolved: "dark" | "light";
  setPreference: (preference: ThemePreference) => void;
}

export const ThemeContext = createContext<ThemeContextValue | null>(null);

export const THEME_STORAGE_KEY = "heikas.theme";

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (value === null) {
    throw new Error("useTheme must be used inside the theme provider");
  }
  return value;
}

export function readStoredPreference(): ThemePreference {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (stored === "dark" || stored === "light" || stored === "system") {
      return stored;
    }
  } catch {
    return "dark";
  }
  return "dark";
}

export function resolvePreference(preference: ThemePreference): "dark" | "light" {
  if (preference !== "system") {
    return preference;
  }
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}
