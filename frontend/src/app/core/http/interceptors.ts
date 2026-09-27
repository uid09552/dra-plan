import { HttpContextToken, HttpErrorResponse, HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { MatSnackBar } from '@angular/material/snack-bar';
import { catchError, throwError } from 'rxjs';

import { Problem } from '../api/models';
import { I18n } from '../i18n/i18n';

/** Sends the UI language so the backend returns localized labels and exports. */
export const languageInterceptor: HttpInterceptorFn = (req, next) => {
  const lang = inject(I18n).lang();
  return next(req.clone({ setHeaders: { 'Accept-Language': lang } }));
};

/** Set on requests where 404 is an expected answer (e.g. "no BIA captured yet"). */
export const EXPECT_NOT_FOUND = new HttpContextToken<boolean>(() => false);

/**
 * Shows RFC 9457 problem details in a snackbar. Validation errors (422) are rethrown untouched so
 * forms and the workflow view can render the individual issues themselves.
 */
export const errorInterceptor: HttpInterceptorFn = (req, next) => {
  const snackBar = inject(MatSnackBar);
  const i18n = inject(I18n);
  return next(req).pipe(
    catchError((error: unknown) => {
      const expected =
        error instanceof HttpErrorResponse &&
        (error.status === 422 || (error.status === 404 && req.context.get(EXPECT_NOT_FOUND)));
      if (error instanceof HttpErrorResponse && !expected) {
        const problem = error.error as Partial<Problem> | null;
        const message =
          error.status === 0
            ? i18n.t('errors.network')
            : (problem?.detail ?? problem?.title ?? i18n.t('errors.generic'));
        snackBar.open(message, 'OK', { duration: 6000 });
      }
      return throwError(() => error);
    }),
  );
};

/** Extracts issue messages from a 422 response. */
export function issuesOf(error: unknown): string[] {
  if (error instanceof HttpErrorResponse && error.status === 422) {
    const problem = error.error as Partial<Problem> | null;
    return (problem?.issues ?? []).map((i) => i.message);
  }
  return [];
}
