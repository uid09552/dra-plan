import { DatePipe } from '@angular/common';
import { Component, inject } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { RouterLink } from '@angular/router';
import { forkJoin, map, of, switchMap } from 'rxjs';

import { Api } from '../../core/api/api';
import { ItService, RecoveryRun } from '../../core/api/models';
import { TranslatePipe } from '../../core/i18n/i18n';

interface ActiveRun {
  service: ItService;
  run: RecoveryRun;
}

/** Active recovery runs across all IT services, with RTO clock and progress. */
@Component({
  selector: 'app-recovery',
  imports: [
    RouterLink,
    DatePipe,
    MatCardModule,
    MatIconModule,
    MatProgressBarModule,
    TranslatePipe,
  ],
  template: `
    <div class="page">
      <h1 class="page-title">{{ 'recovery.title' | t }}</h1>
      <p class="page-subtitle">{{ 'recovery.subtitle' | t }}</p>
      @if (runs.isLoading()) {
        <mat-progress-bar mode="indeterminate" />
      }
      @if (runs.hasValue() && runs.value().length === 0) {
        <mat-card appearance="outlined">
          <mat-card-content class="d-flex align-items-center gap-3">
            <mat-icon class="calm">verified_user</mat-icon>{{ 'recovery.empty' | t }}
          </mat-card-content>
        </mat-card>
      }
      <div class="row g-3">
        @for (r of runs.value() ?? []; track r.run.id) {
          <div class="col-12 col-lg-6">
            <mat-card appearance="outlined" [class.at-risk]="r.run.clock.rtoAtRisk">
              <mat-card-header>
                <mat-icon mat-card-avatar class="alarm">emergency</mat-icon>
                <mat-card-title
                  ><a [routerLink]="['/services', r.service.id]">{{
                    r.service.name
                  }}</a></mat-card-title
                >
                <mat-card-subtitle>
                  <span
                    class="status"
                    [class.error]="r.run.mode === 'real'"
                    [class.info]="r.run.mode === 'test'"
                  >
                    {{ 'recovery.mode.' + r.run.mode | t }}
                  </span>
                  {{ r.run.status }} · {{ r.run.declaredAt | date: 'short' }}
                </mat-card-subtitle>
              </mat-card-header>
              <mat-card-content>
                <div class="row text-center my-2">
                  <div class="col">
                    <div class="metric">{{ r.run.clock.elapsedMinutes }}</div>
                    <div class="muted">
                      {{ 'recovery.elapsed' | t }} ({{ 'common.minutes' | t }})
                    </div>
                  </div>
                  <div class="col">
                    <div class="metric">{{ r.run.clock.serviceRtoMinutes ?? '–' }}</div>
                    <div class="muted">{{ 'recovery.rto' | t }} ({{ 'common.minutes' | t }})</div>
                  </div>
                  <div class="col">
                    <div class="metric">{{ r.run.clock.remainingCriticalPathMinutes }}</div>
                    <div class="muted">{{ 'recovery.remaining' | t }}</div>
                  </div>
                </div>
                @if (r.run.clock.rtoAtRisk) {
                  <div class="status error mb-2">{{ 'recovery.atRisk' | t }}</div>
                }
                <mat-progress-bar mode="determinate" [value]="percent(r.run)" />
                <div class="muted mt-1">
                  {{
                    'recovery.progress'
                      | t
                        : {
                            done: r.run.progress.done + r.run.progress.skipped,
                            total: r.run.progress.total,
                          }
                  }}
                </div>
              </mat-card-content>
            </mat-card>
          </div>
        }
      </div>
    </div>
  `,
  styles: `
    .metric {
      font: var(--mat-sys-headline-medium);
    }
    .alarm {
      color: var(--mat-sys-error);
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .calm {
      color: light-dark(#2e7d32, #81c784);
    }
    .at-risk {
      border-color: var(--mat-sys-error);
    }
    a {
      text-decoration: none;
    }
  `,
})
export class Recovery {
  private readonly api = inject(Api);

  protected readonly runs = rxResource({
    stream: () =>
      this.api.services().pipe(
        map((page) => page.items.filter((s) => s.summary?.activeRecoveryRunId)),
        switchMap((services) =>
          services.length === 0
            ? of<ActiveRun[]>([])
            : forkJoin(
                services.map((service) =>
                  this.api
                    .recoveryRun(service.summary!.activeRecoveryRunId!)
                    .pipe(map((run) => ({ service, run }))),
                ),
              ),
        ),
      ),
  });

  protected percent(run: RecoveryRun): number {
    const p = run.progress;
    return p.total ? ((p.done + p.skipped) / p.total) * 100 : 0;
  }
}
