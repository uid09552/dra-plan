import { Component, computed, input } from '@angular/core';

import { PlanStatus, StepStatus } from '../../core/api/models';
import { TranslatePipe } from '../../core/i18n/i18n';

const PLAN_TONE: Record<PlanStatus, string> = {
  approved: 'ok',
  in_review: 'info',
  draft: 'warn',
  retired: '',
};

@Component({
  selector: 'app-plan-status',
  imports: [TranslatePipe],
  template: `<span class="status" [class]="tone()">{{ status() ?? ('common.none' | t) }}</span>`,
})
export class PlanStatusChip {
  readonly status = input<PlanStatus | undefined>();
  protected readonly tone = computed(() => {
    const s = this.status();
    return s ? PLAN_TONE[s] : '';
  });
}

const STEP_TONE: Record<StepStatus, string> = {
  complete: 'ok',
  in_progress: 'info',
  needs_review: 'warn',
  not_started: '',
};

@Component({
  selector: 'app-step-status',
  imports: [TranslatePipe],
  template: `<span class="status" [class]="tone()">{{ 'workflow.status.' + status() | t }}</span>`,
})
export class StepStatusChip {
  readonly status = input.required<StepStatus>();
  protected readonly tone = computed(() => STEP_TONE[this.status()]);
}
