import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";

import {
  readStoredPreference,
  resolvePreference,
  ThemeContext,
  THEME_STORAGE_KEY,
} from "./themeContext";
import type { ThemePreference } from "./themeContext";

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>(() => readStoredPreference());
  const [resolved, setResolved] = useState<"dark" | "light">(() => resolvePreference(preference));

  useEffect(() => {
    const next = resolvePreference(preference);
    setResolved(next);
    document.documentElement.dataset["theme"] = next;
    document.documentElement.style.colorScheme = next;
    if (preference !== "system") {
      return;
    }
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const listener = () => {
      const updated = media.matches ? "light" : "dark";
      setResolved(updated);
      document.documentElement.dataset["theme"] = updated;
      document.documentElement.style.colorScheme = updated;
    };
    media.addEventListener("change", listener);
    return () => {
      media.removeEventListener("change", listener);
    };
  }, [preference]);

  const setPreference = useCallback((next: ThemePreference) => {
    setPreferenceState(next);
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, next);
    } catch {
      setPreferenceState(next);
    }
  }, []);

  const value = useMemo(
    () => ({ preference, resolved, setPreference }),
    [preference, resolved, setPreference],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}
