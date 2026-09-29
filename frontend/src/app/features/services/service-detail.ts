import { DatePipe } from '@angular/common';
import { Component, computed, inject, input, signal } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatTableModule } from '@angular/material/table';
import { MatTabsModule } from '@angular/material/tabs';
import { MatTooltipModule } from '@angular/material/tooltip';
import { RouterLink } from '@angular/router';

import { Api } from '../../core/api/api';
import { injectMutation } from '../../core/http/mutation';
import { TranslatePipe } from '../../core/i18n/i18n';
import { PlanStatusChip } from './status-chips';
import { ComplianceView } from './tabs/compliance-view';
import { HandbookPreview } from './tabs/handbook-preview';
import { ServiceGuide, ServiceTab } from './tabs/service-guide';

/** Tab order in service-detail.html. */
const TAB_INDEX: Record<ServiceTab | 'guide' | 'plans', number> = {
  guide: 0,
  compliance: 1,
  handbook: 2,
  plans: 3,
};

/**
 * One IT service: the Guide owns plan editing; following tabs review compliance, handbook and versions.
 */
@Component({
  selector: 'app-service-detail',
  imports: [
    RouterLink,
    DatePipe,
    MatTabsModule,
    MatButtonModule,
    MatIconModule,
    MatProgressBarModule,
    MatTableModule,
    MatTooltipModule,
    TranslatePipe,
    PlanStatusChip,
    ServiceGuide,
    ComplianceView,
    HandbookPreview,
  ],
  templateUrl: './service-detail.html',
  styleUrl: './service-detail.scss',
})
export class ServiceDetail {
  /** Route parameter `:id` (component input binding). */
  readonly id = input.required<string>();

  private readonly api = inject(Api);
  protected readonly m = injectMutation();

  protected readonly service = rxResource({
    params: () => this.id(),
    stream: ({ params }) => this.api.service(params),
  });
  protected readonly workflow = rxResource({
    params: () => this.id(),
    stream: ({ params }) => this.api.workflow(params),
  });
  protected readonly plans = rxResource({
    params: () => this.id(),
    stream: ({ params }) => this.api.planVersions(params),
  });

  protected readonly stepNumbers = computed(() =>
    Object.fromEntries((this.workflow.value()?.steps ?? []).map((s) => [s.key, s.number])),
  );
  protected readonly tab = signal(TAB_INDEX.guide);
  /** Incremented after every change so the tab components reload their data. */
  protected readonly reloadKey = signal(0);

  protected readonly planColumns = ['version', 'status', 'submitted', 'review', 'actions'];

  protected openTab(tab: ServiceTab): void {
    this.tab.set(TAB_INDEX[tab]);
  }

  protected submitPlan(): void {
    this.m.run(this.api.submitPlan(this.id()), {
      success: 'plans.submitted',
      done: () => this.reload(),
    });
  }

  protected approve(planId: string): void {
    this.m.run(this.api.approvePlan(planId), {
      success: 'plans.approved',
      done: () => this.reload(),
    });
  }

  /** Downloads the emergency handbook (Markdown) in the UI language. */
  protected export(planId: string, version: number): void {
    this.api.exportMarkdown(planId).subscribe((text) => {
      const url = URL.createObjectURL(new Blob([text], { type: 'text/markdown' }));
      const a = document.createElement('a');
      a.href = url;
      a.download = `dr-plan-${this.service.value()?.name ?? 'service'}-v${version}.md`;
      a.click();
      URL.revokeObjectURL(url);
    });
  }

  protected reload(): void {
    this.service.reload();
    this.workflow.reload();
    this.plans.reload();
    this.reloadKey.update((k) => k + 1);
  }
}
