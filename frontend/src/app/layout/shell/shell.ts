import { BreakpointObserver } from '@angular/cdk/layout';
import { Component, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatDividerModule } from '@angular/material/divider';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';
import { MatMenuModule } from '@angular/material/menu';
import { MatSidenavModule } from '@angular/material/sidenav';
import { MatToolbarModule } from '@angular/material/toolbar';
import { MatTooltipModule } from '@angular/material/tooltip';
import { NavigationEnd, Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { filter, map } from 'rxjs';

import { Api } from '../../core/api/api';
import { I18n, Lang, TranslatePipe } from '../../core/i18n/i18n';
import { Theme, ThemeMode } from '../../core/theme/theme';

interface NavItem {
  path: string;
  icon: string;
  label: string;
}

@Component({
  selector: 'app-shell',
  imports: [
    RouterOutlet,
    RouterLink,
    RouterLinkActive,
    MatSidenavModule,
    MatToolbarModule,
    MatListModule,
    MatIconModule,
    MatButtonModule,
    MatMenuModule,
    MatDividerModule,
    MatTooltipModule,
    TranslatePipe,
  ],
  templateUrl: './shell.html',
  styleUrl: './shell.scss',
})
export class Shell {
  protected readonly i18n = inject(I18n);
  protected readonly theme = inject(Theme);
  private readonly router = inject(Router);

  protected readonly nav: NavItem[] = [
    { path: '/dashboard', icon: 'space_dashboard', label: 'nav.dashboard' },
    { path: '/services', icon: 'lan', label: 'nav.services' },
    { path: '/recovery', icon: 'emergency', label: 'nav.recovery' },
    { path: '/catalog', icon: 'menu_book', label: 'nav.catalog' },
    { path: '/settings', icon: 'settings', label: 'nav.settings' },
  ];

  protected readonly languages: Lang[] = ['en', 'de'];
  protected readonly themes: { mode: ThemeMode; icon: string }[] = [
    { mode: 'system', icon: 'brightness_auto' },
    { mode: 'light', icon: 'light_mode' },
    { mode: 'dark', icon: 'dark_mode' },
  ];

  /** Below 960px the navigation overlays the content and closes after navigating. */
  protected readonly compact = toSignal(
    inject(BreakpointObserver)
      .observe('(max-width: 959.98px)')
      .pipe(map((s) => s.matches)),
    { initialValue: false },
  );
  private readonly userOpened = signal(true);
  private readonly drawerOpen = signal(false);
  protected readonly navOpened = computed(() =>
    this.compact() ? this.drawerOpen() : this.userOpened(),
  );

  protected readonly tenant = toSignal(inject(Api).tenant(), { initialValue: undefined });

  constructor() {
    this.router.events
      .pipe(filter((e) => e instanceof NavigationEnd))
      .subscribe(() => this.drawerOpen.set(false));
  }

  protected toggleNav(): void {
    if (this.compact()) {
      this.drawerOpen.update((o) => !o);
    } else {
      this.userOpened.update((o) => !o);
    }
  }

  protected onDrawerClosed(): void {
    this.drawerOpen.set(false);
  }
}
