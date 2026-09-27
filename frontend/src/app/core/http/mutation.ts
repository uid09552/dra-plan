import { inject, signal } from '@angular/core';
import { MatSnackBar } from '@angular/material/snack-bar';
import { finalize, Observable } from 'rxjs';

import { I18n } from '../i18n/i18n';
import { issuesOf } from './interceptors';

export interface MutationOptions<T> {
  /** i18n key of the success message. */
  success?: string;
  /** Called with the response after success. */
  done?: (value: T) => void;
}

/**
 * Per-component mutation runner (call in a field initializer): tracks `busy` and keeps the
 * validation issues (422) of the last request for inline display. Other errors are shown by the
 * error interceptor.
 */
export function injectMutation() {
  const snack = inject(MatSnackBar);
  const i18n = inject(I18n);
  const busy = signal(false);
  const issues = signal<string[]>([]);

  function run<T>(request: Observable<T>, options: MutationOptions<T> = {}): void {
    busy.set(true);
    issues.set([]);
    request.pipe(finalize(() => busy.set(false))).subscribe({
      next: (value) => {
        if (options.success) {
          snack.open(i18n.t(options.success), undefined, { duration: 3000 });
        }
        options.done?.(value);
      },
      error: (e: unknown) => {
        issues.set(issuesOf(e));
      },
    });
  }

  return { busy: busy.asReadonly(), issues: issues.asReadonly(), run, clear: () => issues.set([]) };
}
