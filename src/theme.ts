import type { GlobalThemeOverrides } from "naive-ui";

// Shared by CSS and Naive UI so forms, menus and page surfaces stay consistent.
export const colors = {
  bg: "#11151c",
  "bg-inset": "#0d1117",
  surface: "#181e27",
  "surface-2": "#232c38",
  border: "rgba(164, 184, 210, 0.14)",
  "border-soft": "rgba(164, 184, 210, 0.08)",
  text: "#e9eef5",
  "text-dim": "#a4b1c3",
  "text-faint": "#8796aa",
  accent: "#6aaceb",
  "accent-bright": "#9acbfa",
  "accent-pressed": "#4f91ce",
  "accent-soft": "rgba(106, 172, 235, 0.12)",
  "accent-border": "rgba(106, 172, 235, 0.3)",
  "on-accent": "#0d2033",
  success: "#7ac9a3",
  "success-soft": "rgba(122, 201, 163, 0.1)",
  "success-border": "rgba(122, 201, 163, 0.25)",
  blue: "#9acbfa",
  red: "#f28b91",
  yellow: "#e8be78",
} as const;

export function applyTheme() {
  for (const [name, color] of Object.entries(colors)) {
    document.documentElement.style.setProperty(`--${name}`, color);
  }
}

export const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: colors.accent,
    primaryColorHover: colors["accent-bright"],
    primaryColorPressed: colors["accent-pressed"],
    primaryColorSuppl: colors["accent-bright"],
    successColor: colors.success,
    warningColor: colors.yellow,
    errorColor: colors.red,
    infoColor: colors.blue,
    bodyColor: colors.bg,
    cardColor: colors.surface,
    modalColor: colors.surface,
    popoverColor: colors["surface-2"],
    inputColor: colors["bg-inset"],
    actionColor: colors.surface,
    tableColor: colors.surface,
    textColorBase: colors.text,
    textColor1: colors.text,
    textColor2: colors["text-dim"],
    textColor3: colors["text-faint"],
    borderColor: colors.border,
    dividerColor: colors["border-soft"],
    borderRadius: "10px",
    fontWeight: "500",
  },
  Button: {
    textColorPrimary: colors["on-accent"],
    textColorHoverPrimary: colors["on-accent"],
    textColorPressedPrimary: colors["on-accent"],
    textColorFocusPrimary: colors["on-accent"],
    borderPrimary: `1px solid ${colors["accent-border"]}`,
  },
  Card: { borderRadius: "14px" },
};
