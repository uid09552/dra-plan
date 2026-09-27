import { Component, inject } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';
import { map } from 'rxjs';

import { Api } from '../../core/api/api';
import { TranslatePipe } from '../../core/i18n/i18n';

/** Profile of the (mocked, dev-mode) user and their tenant memberships. */
@Component({
  selector: 'app-profile',
  imports: [MatCardModule, MatIconModule, MatListModule, TranslatePipe],
  template: `
    <div class="page">
      <h1 class="page-title">{{ 'profile.title' | t }}</h1>
      <p class="page-subtitle">
        <span class="status warn">{{ 'user.devMode' | t }}</span>
      </p>
      <div class="row g-3">
        <div class="col-12 col-lg-6">
          <mat-card appearance="outlined">
            <mat-card-header>
              <mat-icon mat-card-avatar class="avatar">account_circle</mat-icon>
              <mat-card-title>dev-user</mat-card-title>
              <mat-card-subtitle>{{ 'profile.role' | t }}: admin</mat-card-subtitle>
            </mat-card-header>
            <mat-card-content class="pt-3">
              <div class="muted">{{ 'profile.tenant' | t }}</div>
              <div>
                {{ tenant.value()?.name }} <span class="muted">({{ tenant.value()?.slug }})</span>
              </div>
            </mat-card-content>
          </mat-card>
        </div>
        <div class="col-12 col-lg-6">
          <mat-card appearance="outlined">
            <mat-card-header
              ><mat-card-title>{{ 'profile.tenants' | t }}</mat-card-title></mat-card-header
            >
            <mat-card-content>
              <mat-list>
                @for (t of tenants.value() ?? []; track t.id) {
                  <mat-list-item>
                    <mat-icon matListItemIcon>apartment</mat-icon>
                    <span matListItemTitle>{{ t.name }}</span>
                    <span matListItemLine class="muted">{{ t.slug }}</span>
                  </mat-list-item>
                }
              </mat-list>
            </mat-card-content>
          </mat-card>
        </div>
      </div>
    </div>
  `,
  styles: `
    .avatar {
      font-size: 40px;
      width: 40px;
      height: 40px;
      color: var(--mat-sys-primary);
    }
  `,
})
export class Profile {
  private readonly api = inject(Api);
  protected readonly tenant = rxResource({ stream: () => this.api.tenant() });
  protected readonly tenants = rxResource({
    stream: () => this.api.tenants().pipe(map((p) => p.items)),
  });
}
