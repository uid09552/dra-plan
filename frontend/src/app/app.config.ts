import { provideHttpClient, withFetch, withInterceptors } from '@angular/common/http';
import {
  ApplicationConfig,
  inject,
  provideAppInitializer,
  provideBrowserGlobalErrorListeners,
} from '@angular/core';
import { MatIconRegistry } from '@angular/material/icon';
import {
  provideRouter,
  withComponentInputBinding,
  withNavigationErrorHandler,
} from '@angular/router';

import { routes } from './app.routes';
import { errorInterceptor, languageInterceptor, startLogin } from './core/http/interceptors';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(
      routes,
      withComponentInputBinding(),
      // A lazy page chunk failed to load (session expired behind the gateway, or a new deployment
      // replaced the chunks): reload once, which re-authenticates and loads the current version.
      withNavigationErrorHandler(() => {
        startLogin();
      }),
    ),
    provideHttpClient(withFetch(), withInterceptors([languageInterceptor, errorInterceptor])),
    // Self-hosted Material Symbols (works offline during a disaster).
    provideAppInitializer(() => {
      inject(MatIconRegistry).setDefaultFontSetClass('material-symbols-outlined');
    }),
  ],
};
