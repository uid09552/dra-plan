import { Component, computed, inject, input } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';

import { Api } from '../../../core/api/api';
import { ComplianceItem } from '../../../core/api/models';
import { I18n, TranslatePipe } from '../../../core/i18n/i18n';

const TONE: Record<ComplianceItem['status'], string> = {
  fulfilled: 'ok',
  in_progress: 'warn',
  open: 'error',
};

/** Compliance mapping: requirement → what to do → evidence → status. */
@Component({
  selector: 'app-compliance-view',
  imports: [MatIconModule, MatProgressBarModule, TranslatePipe],
  template: `
    @if (items.isLoading()) {
      <mat-progress-bar mode="indeterminate" />
    }
    <div class="d-flex flex-wrap gap-2 mb-3">
      @for (f of summary(); track f.framework) {
        <span class="status info">{{ f.framework }}: {{ f.fulfilled }}/{{ f.total }}</span>
      }
    </div>
    <div class="table-scroll">
      <table class="compliance">
        <thead>
          <tr>
            <th>{{ 'compliance.requirement' | t }}</th>
            <th>{{ 'compliance.whatToDo' | t }}</th>
            <th>{{ 'compliance.evidence' | t }}</th>
            <th>{{ 'common.status' | t }}</th>
          </tr>
        </thead>
        <tbody>
          @for (item of items.value() ?? []; track item.id) {
            <tr>
              <td>
                <div class="muted small">{{ item.framework }} · {{ item.reference }}</div>
                <strong>{{ item.title }}</strong>
              </td>
              <td>{{ item.whatToDo }}</td>
              <td>{{ item.evidence }}</td>
              <td>
                <span class="status" [class]="tone(item)">{{
                  'compliance.status.' + item.status | t
                }}</span>
                @if (item.blockingIssues > 0) {
                  <div class="muted small mt-1">
                    {{ 'workflow.gateBlocked' | t: { count: item.blockingIssues } }}
                  </div>
                }
                <div class="muted small mt-1">
                  {{ 'compliance.steps' | t }}: {{ stepList(item) }}
                </div>
              </td>
            </tr>
          }
        </tbody>
      </table>
    </div>
  `,
  styles: `
    .compliance {
      border-collapse: collapse;
      width: 100%;
      min-width: 720px;
    }
    th,
    td {
      border-bottom: 1px solid var(--mat-sys-outline-variant);
      padding: 8px;
      text-align: left;
      vertical-align: top;
    }
    th {
      font: var(--mat-sys-label-large);
      color: var(--mat-sys-on-surface-variant);
    }
    .small {
      font: var(--mat-sys-body-small);
    }
  `,
})
export class ComplianceView {
  readonly serviceId = input.required<string>();
  readonly reloadKey = input(0);
  /** Workflow step key → number, for the "implemented by steps" hint. */
  readonly stepNumbers = input<Partial<Record<string, number>>>({});

  private readonly api = inject(Api);
  private readonly i18n = inject(I18n);

  protected readonly items = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey(), lang: this.i18n.lang() }),
    stream: ({ params }) => this.api.compliance(params.id),
  });

  protected readonly summary = computed(() => {
    const byFramework = new Map<string, { framework: string; fulfilled: number; total: number }>();
    for (const item of this.items.value() ?? []) {
      const f = byFramework.get(item.framework) ?? {
        framework: item.framework,
        fulfilled: 0,
        total: 0,
      };
      f.total++;
      if (item.status === 'fulfilled') {
        f.fulfilled++;
      }
      byFramework.set(item.framework, f);
    }
    return [...byFramework.values()];
  });

  protected tone(item: ComplianceItem): string {
    return TONE[item.status];
  }

  protected stepList(item: ComplianceItem): string {
    const numbers = this.stepNumbers();
    return item.steps.map((s) => numbers[s] ?? s).join(', ');
  }
}
