import { useState, useEffect, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ACCENTS, themeById } from "../themes";
import type { ThemeMode } from "../types";

const THEME_STORAGE_KEY = "theme";
const ACCENT_STORAGE_KEY = "accent";

function getSystemBase(): "light" | "dark" {
  if (typeof window !== "undefined" && window.matchMedia) {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return "dark";
}

function getStoredTheme(): ThemeMode {
  if (typeof window !== "undefined") {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    if (stored === "system") return "system";
    // Accepts both legacy values ("light"/"dark") and preset theme ids.
    if (stored && themeById(stored)) return stored;
  }
  return "system";
}

function getStoredAccent(): string {
  if (typeof window !== "undefined") {
    const stored = localStorage.getItem(ACCENT_STORAGE_KEY);
    if (stored && ACCENTS.some((a) => a.id === stored)) return stored;
  }
  return "auto";
}

/** "system" follows the OS between the built-in dark/light pair. */
function resolveThemeId(mode: ThemeMode): string {
  return mode === "system" ? getSystemBase() : mode;
}

async function applyTheme(mode: ThemeMode, accent: string) {
  const def = themeById(resolveThemeId(mode)) ?? themeById("dark")!;
  const root = document.documentElement;

  root.setAttribute("data-theme", def.id);
  // Drives base-dependent CSS (accent override variants, light shadows).
  root.setAttribute("data-base", def.base);
  if (accent === "auto") {
    root.removeAttribute("data-accent");
  } else {
    root.setAttribute("data-accent", accent);
  }

  // Native titlebar: follow the system, or pin to the theme's base mode.
  try {
    const tauriTheme = mode === "system" ? null : def.base;
    await getCurrentWindow().setTheme(tauriTheme);
  } catch (e) {
    console.error("Failed to set window theme:", e);
  }
}

export function useTheme() {
  const [themeMode, setThemeMode] = useState<ThemeMode>(getStoredTheme);
  const [accent, setAccentState] = useState<string>(getStoredAccent);

  useEffect(() => {
    applyTheme(themeMode, accent);
    localStorage.setItem(THEME_STORAGE_KEY, themeMode);
    localStorage.setItem(ACCENT_STORAGE_KEY, accent);
  }, [themeMode, accent]);

  // Track OS appearance changes while in system mode
  useEffect(() => {
    if (themeMode !== "system") return;

    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => {
      applyTheme("system", accent);
    };

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, [themeMode, accent]);

  const setTheme = useCallback((mode: ThemeMode) => {
    setThemeMode(mode);
  }, []);

  const setAccent = useCallback((value: string) => {
    setAccentState(value);
  }, []);

  return {
    themeMode,
    setTheme,
    accent,
    setAccent,
    effectiveTheme: (themeById(resolveThemeId(themeMode)) ?? themeById("dark")!)
      .base,
  };
}
