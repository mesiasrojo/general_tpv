export const themes = ['light_theme', 'dark_them'] as const;
export type Theme = (typeof themes)[number];
export const defaultTheme: Theme = 'light_theme';

const storageKey = 'general-tpv-theme';

export function storedTheme(): Theme {
  try {
    const value = localStorage.getItem(storageKey);
    if (value === 'light_theme' || value === 'dark_them') {
      return value;
    }
  } catch {
    // localStorage can be unavailable in private or restricted contexts.
  }
  return defaultTheme;
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(storageKey, theme);
  } catch {
    // Persistence is optional; the visual theme still applies.
  }
}

export function nextTheme(theme: Theme): Theme {
  return theme === 'light_theme' ? 'dark_them' : 'light_theme';
}
