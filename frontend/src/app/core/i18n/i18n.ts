import { DOCUMENT, Injectable, Pipe, PipeTransform, effect, inject, signal } from '@angular/core';

import { readPref, writePref } from '../prefs/storage';
import { de } from './de';
import { en } from './en';

export type Lang = 'en' | 'de';

const DICTIONARIES: Record<Lang, unknown> = { en, de };

/** Browser language decides the default (German for any `de-*`), unless the user picked one. */
export function detectLanguage(stored: string | null, browser: readonly string[]): Lang {
  if (stored === 'en' || stored === 'de') {
    return stored;
  }
  return browser
    .find((l) => /^(de|en)\b/i.test(l))
    ?.toLowerCase()
    .startsWith('de')
    ? 'de'
    : 'en';
}

/**
 * Runtime translations (English/German) with signal-based switching. Angular's built-in i18n is
 * compile-time only, so it cannot switch languages at runtime.
 */
@Injectable({ providedIn: 'root' })
export class I18n {
  private readonly document = inject(DOCUMENT);
  readonly lang = signal<Lang>(
    detectLanguage(readPref('lang'), navigator.languages ?? [navigator.language]),
  );

  constructor() {
    effect(() => {
      this.document.documentElement.lang = this.lang();
    });
  }

  use(lang: Lang): void {
    this.lang.set(lang);
    writePref('lang', lang);
  }

  /** Looks up a dotted key (e.g. `nav.dashboard`) and fills `{placeholders}`. */
  t(key: string, params?: Record<string, string | number>): string {
    const text = lookup(DICTIONARIES[this.lang()], key) ?? lookup(en, key) ?? key;
    return params
      ? text.replace(/\{(\w+)\}/g, (_, p: string) => String(params[p] ?? `{${p}}`))
      : text;
  }
}

function lookup(dict: unknown, key: string): string | undefined {
  const value = key
    .split('.')
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === 'object' ? (node as Record<string, unknown>)[part] : undefined,
      dict,
    );
  return typeof value === 'string' ? value : undefined;
}

/** `{{ 'nav.dashboard' | t }}` — re-evaluates when the language signal changes. */
@Pipe({ name: 't', pure: false })
export class TranslatePipe implements PipeTransform {
  private readonly i18n = inject(I18n);

  transform(key: string, params?: Record<string, string | number>): string {
    return this.i18n.t(key, params);
  }
}
