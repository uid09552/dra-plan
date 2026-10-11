import { CdkDrag, CdkDragDrop, CdkDragHandle, CdkDropList } from '@angular/cdk/drag-drop';
import { Component, computed, inject, input, linkedSignal, output, signal } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatTooltipModule } from '@angular/material/tooltip';
import { Observable, filter, forkJoin, map, switchMap, tap } from 'rxjs';

import { Api } from '../../../core/api/api';
import {
  Issue,
  Microservice,
  RUNBOOK_PHASES,
  RecoveryObjective,
  Role,
  RunbookDetail,
  RunbookPhase,
  RunbookStep,
  Scenario,
} from '../../../core/api/models';
import { injectMutation } from '../../../core/http/mutation';
import { I18n, TranslatePipe } from '../../../core/i18n/i18n';
import { ContextMenu, MenuItem } from '../../../core/ui/context-menu';
import { EditResult, FieldDef, openConfirm, openEdit } from '../../../core/ui/edit-dialog';
import {
  dependencyOptions,
  effectiveRto,
  issuesOf,
  movedOrder,
  stepBody,
  stepRows,
} from './runbook-model';

interface EditorData {
  runbook: RunbookDetail;
  roles: Role[];
  objectives: RecoveryObjective[];
}

/**
 * Step editor of one runbook: steps in execution order with phase separators, dialogs for
 * add/edit/insert/delete, drag and drop or "move up/down" to reorder, gate issues per step.
 */
@Component({
  selector: 'app-runbook-editor',
  imports: [
    CdkDropList,
    CdkDrag,
    CdkDragHandle,
    MatButtonModule,
    MatIconModule,
    MatProgressBarModule,
    MatTooltipModule,
    TranslatePipe,
    ContextMenu,
  ],
  template: `
    <div class="d-flex flex-wrap align-items-center gap-2 mt-3 mb-2">
      <button mat-button (click)="back.emit()">
        <mat-icon>arrow_back</mat-icon>{{ 'runbooks.back' | t }}
      </button>
      <span class="flex-grow-1"></span>
      <button mat-flat-button (click)="add()" [disabled]="m.busy() || !data()">
        <mat-icon>add</mat-icon>{{ 'runbooks.addStep' | t }}
      </button>
    </div>
    @if (data(); as d) {
      <header class="head">
        <h3 class="title">{{ d.runbook.title }}</h3>
        <div class="d-flex flex-wrap align-items-center gap-2">
          <span class="status info">{{ scenarioTitle() }}</span>
          <span class="status info">{{ componentName() }}</span>
          <span class="nowrap">
            {{ 'runbooks.criticalPath' | t: { minutes: d.runbook.criticalPathMinutes } }}
            @if (rto() !== undefined) {
              · {{ 'runbooks.rto' | t: { minutes: rto()! } }}
            }
          </span>
          @if (d.runbook.exceedsRto) {
            <span class="status error" [matTooltip]="'runbooks.exceedsRtoHint' | t">{{
              'runbooks.exceedsRto' | t
            }}</span>
          }
        </div>
        @if (d.runbook.description) {
          <p class="muted mb-0">{{ d.runbook.description }}</p>
        }
      </header>
    }
    <p class="muted small">{{ 'runbooks.editorHint' | t }}</p>
    @for (e of m.issues(); track e) {
      <div class="status error mb-2" role="alert">{{ e }}</div>
    }
    @if (resource.isLoading() && !data()) {
      <mat-progress-bar mode="indeterminate" />
    }
    <div
      class="steps"
      cdkDropList
      (cdkDropListDropped)="drop($event)"
      [cdkDropListDisabled]="m.busy()"
    >
      @for (row of rows(); track row.step.id; let i = $index) {
        @if (row.phaseStart) {
          <div class="phase">{{ 'runbooks.phase.' + row.phaseStart | t }}</div>
        }
        <div
          class="step"
          cdkDrag
          tabindex="0"
          [attr.aria-label]="row.step.seq + '. ' + row.step.title"
          (dblclick)="edit(row.step)"
          (keydown.enter)="edit(row.step)"
          (contextmenu)="menu.open($event, stepMenu(row.step, i))"
        >
          <mat-icon class="handle" cdkDragHandle [attr.aria-hidden]="true">drag_indicator</mat-icon>
          <span class="seq">{{ row.step.seq }}</span>
          <div class="body">
            <div class="d-flex flex-wrap align-items-center gap-2">
              <strong>{{ row.step.title }}</strong>
              <span class="muted">{{ roleName(row.step.ownerRoleId) }}</span>
              @if (row.step.expectedDurationMinutes != null) {
                <span class="muted nowrap">{{ row.step.expectedDurationMinutes }} min</span>
              }
              @if (row.step.isDecisionPoint) {
                <span class="status warn">{{ 'runbooks.decisionPoint' | t }}</span>
              }
              @if (row.step.requiresAuthorizationRoleId) {
                <span class="status info">{{
                  'runbooks.approvalBy'
                    | t: { role: roleName(row.step.requiresAuthorizationRoleId) }
                }}</span>
              }
              @for (i of stepIssues(row.step); track i.ruleId) {
                <mat-icon
                  class="issue"
                  [class.blocking]="i.severity === 'blocking'"
                  [matTooltip]="i.message"
                  [attr.aria-label]="i.message"
                  >error</mat-icon
                >
              }
            </div>
            @if (row.step.prerequisites) {
              <div class="detail">
                <span class="label">{{ 'runbooks.field.prerequisites' | t }}:</span>
                {{ row.step.prerequisites }}
              </div>
            }
            @if (row.step.verification) {
              <div class="detail">
                <span class="label">{{ 'runbooks.field.verification' | t }}:</span>
                {{ row.step.verification }}
              </div>
            }
            @if (row.step.evidenceRequired) {
              <div class="detail">
                <span class="label">{{ 'runbooks.field.evidenceRequired' | t }}:</span>
                {{ row.step.evidenceRequired }}
              </div>
            }
          </div>
          <button
            mat-icon-button
            (click)="menu.open($event, stepMenu(row.step, i))"
            [attr.aria-label]="'common.actions' | t"
          >
            <mat-icon>more_vert</mat-icon>
          </button>
        </div>
      } @empty {
        @if (data()) {
          <p class="muted">{{ 'runbooks.noSteps' | t }}</p>
        }
      }
    </div>
    <app-context-menu #menu />
  `,
  styles: `
    .head {
      margin-bottom: 8px;
    }
    .title {
      font: var(--mat-sys-title-medium);
      margin: 0 0 4px;
    }
    .small {
      font: var(--mat-sys-body-small);
    }
    .nowrap {
      white-space: nowrap;
    }
    .phase {
      font: var(--mat-sys-label-large);
      color: var(--mat-sys-primary);
      border-bottom: 2px solid var(--mat-sys-primary);
      margin: 16px 0 4px;
      padding-bottom: 2px;
    }
    .step {
      display: flex;
      align-items: flex-start;
      gap: 8px;
      padding: 6px 8px;
      border-bottom: 1px solid var(--mat-sys-outline-variant);
      background: var(--mat-sys-surface);
    }
    .step:hover,
    .step:focus-visible {
      background: var(--mat-sys-surface-container-low);
    }
    .handle {
      cursor: grab;
      color: var(--mat-sys-on-surface-variant);
      margin-top: 2px;
    }
    .seq {
      font: var(--mat-sys-label-large);
      min-width: 24px;
      margin-top: 4px;
    }
    .body {
      flex: 1;
      min-width: 0;
      padding-top: 2px;
    }
    .detail {
      font: var(--mat-sys-body-small);
      color: var(--mat-sys-on-surface-variant);
      white-space: pre-line;
    }
    .label {
      font-weight: 500;
    }
    .issue {
      color: var(--mat-sys-tertiary);
    }
    .issue.blocking {
      color: var(--mat-sys-error);
    }
    .cdk-drag-preview {
      box-shadow: var(--mat-sys-level3);
    }
    .cdk-drag-placeholder {
      opacity: 0.3;
    }
  `,
})
export class RunbookEditor {
  readonly runbookId = input.required<string>();
  readonly reloadKey = input(0);
  readonly scenarios = input<Scenario[]>([]);
  readonly components = input<Microservice[]>([]);
  /** Gate issues of the runbooks workflow step. */
  readonly issues = input<Issue[]>([]);
  readonly back = output<void>();
  readonly changed = output<void>();

  private readonly api = inject(Api);
  private readonly i18n = inject(I18n);
  private readonly dialog = inject(MatDialog);
  protected readonly m = injectMutation();
  /** Reloads the stored state, for example after a rejected reorder. */
  private readonly localKey = signal(0);

  protected readonly resource = rxResource({
    params: () => ({ id: this.runbookId(), key: this.reloadKey(), local: this.localKey() }),
    stream: ({ params }) =>
      this.api.runbook(params.id).pipe(
        switchMap((runbook) =>
          forkJoin({
            roles: this.api.roles(),
            objectives: this.api.objectives(runbook.microserviceId),
          }).pipe(map((rest) => ({ runbook, ...rest }))),
        ),
      ),
  });
  protected readonly data = linkedSignal<EditorData | undefined, EditorData | undefined>({
    source: () => this.resource.value(),
    computation: (value, previous) => value ?? previous?.value,
  });
  /** Steps in display order; changed optimistically while a reorder is saved. */
  private readonly steps = linkedSignal(() =>
    [...(this.data()?.runbook.steps ?? [])].sort((a, b) => a.seq - b.seq),
  );
  protected readonly rows = computed(() => stepRows(this.steps()));
  protected readonly rto = computed(() => {
    const rb = this.data()?.runbook;
    return rb ? effectiveRto(this.data()!.objectives, rb.microserviceId, rb.scenarioId) : undefined;
  });
  protected readonly scenarioTitle = computed(
    () => this.scenarios().find((s) => s.id === this.data()?.runbook.scenarioId)?.title ?? '–',
  );
  protected readonly componentName = computed(
    () => this.components().find((c) => c.id === this.data()?.runbook.microserviceId)?.name ?? '–',
  );

  protected roleName(id: string | null | undefined): string {
    return (id && this.data()?.roles.find((r) => r.id === id)?.name) || '–';
  }

  protected stepIssues(step: RunbookStep): Issue[] {
    return issuesOf(this.issues(), 'runbook_step', step.id);
  }

  protected stepMenu(step: RunbookStep, index: number): MenuItem[] {
    const last = this.steps().length - 1;
    return [
      { icon: 'edit', label: 'common.edit', run: () => this.edit(step) },
      {
        icon: 'vertical_align_top',
        label: 'runbooks.insertAbove',
        run: () => this.add(step.seq, step.phase),
      },
      {
        icon: 'vertical_align_bottom',
        label: 'runbooks.insertBelow',
        run: () => this.add(step.seq + 1, step.phase),
      },
      {
        icon: 'arrow_upward',
        label: 'runbooks.moveUp',
        disabled: index === 0,
        divider: true,
        run: () => this.move(index, index - 1),
      },
      {
        icon: 'arrow_downward',
        label: 'runbooks.moveDown',
        disabled: index === last,
        run: () => this.move(index, index + 1),
      },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        run: () => this.remove(step),
      },
    ];
  }

  protected drop(event: CdkDragDrop<unknown>): void {
    if (event.previousIndex !== event.currentIndex) {
      this.move(event.previousIndex, event.currentIndex);
    }
  }

  /** Adds a step at `position` (1-based; default: end). */
  protected add(position?: number, phase?: RunbookPhase): void {
    const steps = this.steps();
    const seq = position ?? steps.length + 1;
    const last = steps[steps.length - 1];
    const request = openEdit(this.dialog, {
      title: 'runbooks.addStep',
      submit: 'common.add',
      fields: this.fields(seq),
      value: { phase: phase ?? last?.phase ?? 'activation', dependsOnStepIds: [] },
    }).pipe(
      filter((v): v is EditResult => !!v),
      switchMap((v) => this.api.addRunbookStep(this.runbookId(), { ...stepBody(v), position })),
    );
    this.save(request);
  }

  protected edit(step: RunbookStep): void {
    const request = openEdit(this.dialog, {
      title: 'runbooks.editStep',
      fields: this.fields(step.seq),
      value: {
        phase: step.phase,
        title: step.title,
        instructions: step.instructions ?? '',
        ownerRoleId: step.ownerRoleId ?? null,
        expectedDurationMinutes: step.expectedDurationMinutes ?? null,
        verification: step.verification ?? '',
        prerequisites: step.prerequisites ?? '',
        evidenceRequired: step.evidenceRequired ?? '',
        dependsOnStepIds: step.dependsOnStepIds,
        isDecisionPoint: step.isDecisionPoint,
        requiresAuthorizationRoleId: step.requiresAuthorizationRoleId ?? null,
      },
    }).pipe(
      filter((v): v is EditResult => !!v),
      switchMap((v) => this.api.updateRunbookStep(step.id, step.version, stepBody(v))),
    );
    this.save(request);
  }

  private move(from: number, to: number): void {
    const before = this.steps();
    const ids = movedOrder(before, from, to);
    const byId = new Map(before.map((s) => [s.id, s]));
    this.steps.set(ids.map((id, i) => ({ ...byId.get(id)!, seq: i + 1 })));
    this.save(
      this.api.reorderRunbookSteps(this.runbookId(), ids).pipe(
        // Rejected (e.g. a dependency would come later): show the stored order again.
        tap({ error: () => this.localKey.update((k) => k + 1) }),
      ),
    );
  }

  private remove(step: RunbookStep): void {
    const request = openConfirm(this.dialog, {
      title: 'runbooks.deleteStepConfirm',
      message: step.title,
      confirm: 'common.delete',
    }).pipe(
      filter((ok) => !!ok),
      switchMap(() => this.api.deleteRunbookStep(step.id)),
    );
    this.save(request, 'common.deleted');
  }

  private fields(seq: number): FieldDef[] {
    const t = (key: string) => this.i18n.t(key);
    const roles = [
      { value: null, label: '–' },
      ...(this.data()?.roles ?? []).map((r) => ({ value: r.id, label: r.name })),
    ];
    return [
      {
        key: 'phase',
        label: 'runbooks.field.phase',
        type: 'select',
        required: true,
        options: RUNBOOK_PHASES.map((p) => ({ value: p, label: t(`runbooks.phase.${p}`) })),
      },
      { key: 'title', label: 'runbooks.field.title', type: 'text', required: true },
      {
        key: 'instructions',
        label: 'runbooks.field.instructions',
        type: 'textarea',
        hint: t('runbooks.field.instructionsHint'),
      },
      { key: 'ownerRoleId', label: 'runbooks.field.owner', type: 'select', options: roles },
      {
        key: 'expectedDurationMinutes',
        label: 'runbooks.field.duration',
        type: 'number',
        min: 0,
        suffix: 'min',
      },
      {
        key: 'prerequisites',
        label: 'runbooks.field.prerequisites',
        type: 'textarea',
        hint: t('runbooks.field.prerequisitesHint'),
      },
      {
        key: 'verification',
        label: 'runbooks.field.verification',
        type: 'textarea',
        hint: t('runbooks.field.verificationHint'),
      },
      {
        key: 'evidenceRequired',
        label: 'runbooks.field.evidenceRequired',
        type: 'textarea',
        hint: t('runbooks.field.evidenceRequiredHint'),
      },
      {
        key: 'dependsOnStepIds',
        label: 'runbooks.field.dependsOn',
        type: 'multiselect',
        hint: t('runbooks.field.dependsOnHint'),
        options: dependencyOptions(this.steps(), seq).map((s) => ({
          value: s.id,
          label: `${s.seq}. ${s.title}`,
        })),
      },
      { key: 'isDecisionPoint', label: 'runbooks.field.decisionPoint', type: 'checkbox' },
      {
        key: 'requiresAuthorizationRoleId',
        label: 'runbooks.field.approvalRole',
        type: 'select',
        hint: t('runbooks.field.approvalRoleHint'),
        options: roles,
      },
    ];
  }

  private save(request: Observable<unknown>, success = 'common.saved'): void {
    this.m.run(request, { success, done: () => this.changed.emit() });
  }
}
