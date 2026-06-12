// Theme registry. A theme is a named set of design-token overrides defined
// in themes.css (keyed by [data-theme="<id>"]); this module is the single
// source of truth for which themes exist and what the picker shows.
//
// `base` drives light/dark-dependent pieces that CSS can't express per theme
// (the native titlebar, accent override variants via [data-base]).

export interface ThemeDef {
  id: string;
  label: string;
  base: "light" | "dark";
  /** Swatch colors for the picker card. */
  preview: {
    bg: string;
    surface: string;
    text: string;
    accent: string;
    /** A few representative state colors, shown as dots. */
    states: [string, string, string];
  };
}

export const THEMES: ThemeDef[] = [
  {
    id: "dark",
    label: "RBitt Dark",
    base: "dark",
    preview: {
      bg: "#101012",
      surface: "#1d1d21",
      text: "#ececef",
      accent: "#6a6ef5",
      states: ["#3ad6c5", "#3ecf8e", "#e9b949"],
    },
  },
  {
    id: "light",
    label: "RBitt Light",
    base: "light",
    preview: {
      bg: "#f6f6f8",
      surface: "#ffffff",
      text: "#1c1c21",
      accent: "#5559e6",
      states: ["#0d9488", "#18935e", "#ad8211"],
    },
  },
  {
    id: "catppuccin-mocha",
    label: "Catppuccin Mocha",
    base: "dark",
    preview: {
      bg: "#1e1e2e",
      surface: "#313244",
      text: "#cdd6f4",
      accent: "#cba6f7",
      states: ["#94e2d5", "#a6e3a1", "#f9e2af"],
    },
  },
  {
    id: "catppuccin-latte",
    label: "Catppuccin Latte",
    base: "light",
    preview: {
      bg: "#eff1f5",
      surface: "#ffffff",
      text: "#4c4f69",
      accent: "#8839ef",
      states: ["#179299", "#40a02b", "#df8e1d"],
    },
  },
  {
    id: "tokyo-night",
    label: "Tokyo Night",
    base: "dark",
    preview: {
      bg: "#1a1b26",
      surface: "#292e42",
      text: "#c0caf5",
      accent: "#7aa2f7",
      states: ["#1abc9c", "#9ece6a", "#e0af68"],
    },
  },
  {
    id: "nord",
    label: "Nord",
    base: "dark",
    preview: {
      bg: "#2e3440",
      surface: "#3b4252",
      text: "#eceff4",
      accent: "#88c0d0",
      states: ["#8fbcbb", "#a3be8c", "#ebcb8b"],
    },
  },
  {
    id: "dracula",
    label: "Dracula",
    base: "dark",
    preview: {
      bg: "#282a36",
      surface: "#343746",
      text: "#f8f8f2",
      accent: "#bd93f9",
      states: ["#8be9fd", "#50fa7b", "#f1fa8c"],
    },
  },
  {
    id: "gruvbox-dark",
    label: "Gruvbox Dark",
    base: "dark",
    preview: {
      bg: "#282828",
      surface: "#3c3836",
      text: "#ebdbb2",
      accent: "#83a598",
      states: ["#8ec07c", "#b8bb26", "#fabd2f"],
    },
  },
];

export function themeById(id: string): ThemeDef | undefined {
  return THEMES.find((t) => t.id === id);
}

export interface AccentDef {
  id: string;
  label: string;
  /** Swatch color; null = follow the theme's own accent. */
  color: string | null;
}

// Accent overrides apply on top of any theme via [data-accent] blocks in
// themes.css (with [data-base] variants so contrast holds on light themes).
export const ACCENTS: AccentDef[] = [
  { id: "auto", label: "Theme default", color: null },
  { id: "indigo", label: "Indigo", color: "#6a6ef5" },
  { id: "blue", label: "Blue", color: "#3f8cf3" },
  { id: "violet", label: "Violet", color: "#8b5cf6" },
  { id: "magenta", label: "Magenta", color: "#d946ef" },
  { id: "rose", label: "Rose", color: "#f43f5e" },
  { id: "orange", label: "Orange", color: "#f97316" },
  { id: "green", label: "Green", color: "#22c55e" },
  { id: "teal", label: "Teal", color: "#14b8a6" },
];
