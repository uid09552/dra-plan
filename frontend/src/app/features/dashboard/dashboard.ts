import { DatePipe } from '@angular/common';
import { Component, computed, inject } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { RouterLink } from '@angular/router';
import { map } from 'rxjs';

import { Api } from '../../core/api/api';
import { NextAction } from '../../core/api/models';
import { I18n, TranslatePipe } from '../../core/i18n/i18n';
import { scoreTone } from '../services/tabs/service-guide';

/** Cockpit home: "How ready are we?" across all IT services. */
@Component({
  selector: 'app-dashboard',
  imports: [
    RouterLink,
    DatePipe,
    MatCardModule,
    MatIconModule,
    MatButtonModule,
    MatProgressBarModule,
    TranslatePipe,
  ],
  templateUrl: './dashboard.html',
  styleUrl: './dashboard.scss',
})
export class Dashboard {
  private readonly api = inject(Api);
  private readonly i18n = inject(I18n);

  protected readonly readiness = rxResource({
    params: () => this.i18n.lang(),
    stream: () => this.api.readiness(),
  });
  /** Active recoveries come from the service summaries. */
  protected readonly activeRuns = rxResource({
    stream: () =>
      this.api.services().pipe(map((p) => p.items.filter((s) => s.summary?.activeRecoveryRunId))),
  });

  protected readonly scoreTone = scoreTone;
  protected readonly severityIcon: Record<NextAction['severity'], string> = {
    blocking: 'block',
    warning: 'warning',
    info: 'arrow_forward',
  };

  protected readonly kpis = computed(() => {
    const r = this.readiness.value();
    if (!r) {
      return [];
    }
    return [
      { icon: 'report', label: 'readiness.criticalGaps', value: `${r.critical}`, tone: 'error' },
      { icon: 'warning', label: 'readiness.attention', value: `${r.attention}`, tone: 'warn' },
      {
        icon: 'task_alt',
        label: 'readiness.completedSteps',
        value: `${r.completed}/${r.total}`,
        tone: 'ok',
      },
      {
        icon: 'timer',
        label: 'readiness.rtoCompliance',
        value: r.rtoCompliance === undefined ? '–' : `${r.rtoCompliance}%`,
        tone: '',
      },
      {
        icon: 'event_busy',
        label: 'readiness.plansRequiringReview',
        value: `${r.plansRequiringReview}`,
        tone: 'warn',
      },
      {
        icon: 'science',
        label: 'readiness.untestedScenarios',
        value: `${r.untestedScenarios}`,
        tone: 'warn',
      },
    ];
  });
}
