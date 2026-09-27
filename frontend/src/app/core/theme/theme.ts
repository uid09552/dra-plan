import { DOCUMENT, Injectable, effect, inject, signal } from '@angular/core';

import { readPref, writePref } from '../prefs/storage';

export type ThemeMode = 'system' | 'light' | 'dark';

/** Dark mode: follows the OS by default; `light`/`dark` pin the Material `color-scheme`. */
@Injectable({ providedIn: 'root' })
export class Theme {
  private readonly document = inject(DOCUMENT);
  readonly mode = signal<ThemeMode>(parseMode(readPref('theme')));

  constructor() {
    effect(() => {
      const root = this.document.documentElement.classList;
      root.remove('theme-light', 'theme-dark');
      const mode = this.mode();
      if (mode !== 'system') {
        root.add(`theme-${mode}`);
      }
    });
  }

  use(mode: ThemeMode): void {
    this.mode.set(mode);
    writePref('theme', mode);
  }

  icon(): string {
    return { system: 'brightness_auto', light: 'light_mode', dark: 'dark_mode' }[this.mode()];
  }
}

function parseMode(value: string | null): ThemeMode {
  return value === 'light' || value === 'dark' ? value : 'system';
}
