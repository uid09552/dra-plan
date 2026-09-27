/**
 * Per-browser preferences. Storage can be unavailable (private mode, blocked site data), so every
 * access is guarded and callers always get a usable default.
 */
export function readPref(key: string): string | null {
  try {
    return localStorage.getItem(`dra.${key}`);
  } catch {
    return null;
  }
}

export function writePref(key: string, value: string): void {
  try {
    localStorage.setItem(`dra.${key}`, value);
  } catch {
    // Preference is kept for this session only.
  }
}
