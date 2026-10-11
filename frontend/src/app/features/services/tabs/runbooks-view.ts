import { Component, computed, inject, input, linkedSignal, output } from '@angular/core';
import { rxResource, toSignal } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatTooltipModule } from '@angular/material/tooltip';
import { ActivatedRoute, Router } from '@angular/router';
import { Observable, filter, forkJoin, map, switchMap } from 'rxjs';

import { Api } from '../../../core/api/api';
import { Issue, Microservice, RecoveryStrategy, Runbook, Scenario } from '../../../core/api/models';
import { injectMutation } from '../../../core/http/mutation';
import { I18n, TranslatePipe } from '../../../core/i18n/i18n';
import { ContextMenu, MenuItem } from '../../../core/ui/context-menu';
import { EditResult, FieldDef, openConfirm, openEdit } from '../../../core/ui/edit-dialog';
import { RunbookEditor } from './runbook-editor';
import { RunbookRow, issuesOf, runbookGroups } from './runbook-model';

interface RunbooksData {
  runbooks: Runbook[];
  components: Microservice[];
  scenarios: Scenario[];
  strategies: RecoveryStrategy[];
  issues: Issue[];
}

/**
 * Runbooks of a service grouped by DR scenario (workflow step 9), one row per affected component.
 * The open runbook is kept in the `runbook` query parameter and edited in the step editor.
 */
@Component({
  selector: 'app-runbooks-view',
  imports: [
    MatButtonModule,
    MatIconModule,
    MatProgressBarModule,
    MatTooltipModule,
    TranslatePipe,
    ContextMenu,
    RunbookEditor,
  ],
  template: `
    @if (openId(); as id) {
      <app-runbook-editor
        [runbookId]="id"
        [reloadKey]="reloadKey()"
        [scenarios]="data()?.scenarios ?? []"
        [components]="data()?.components ?? []"
        [issues]="data()?.issues ?? []"
        (back)="open(null)"
        (changed)="changed.emit()"
      />
    } @else {
      <h3 class="section-title">{{ 'runbooks.title' | t }}</h3>
      <p class="muted small">{{ 'runbooks.hint' | t }}</p>
      @for (e of m.issues(); track e) {
        <div class="status error mb-2">{{ e }}</div>
      }
      @if (resource.isLoading() && !data()) {
        <mat-progress-bar mode="indeterminate" />
      }
      @for (g of groups(); track g.scenario.id) {
        <section class="group" [attr.aria-label]="g.scenario.title">
          <h4 class="group-title">{{ g.scenario.title }}</h4>
          <ul class="rows">
            @for (row of g.rows; track row.component.id) {
              @for (rb of row.runbooks; track rb.id) {
                <li
                  class="row-item"
                  tabindex="0"
                  (click)="open(rb.id)"
                  (keydown.enter)="open(rb.id)"
                  (contextmenu)="menu.open($event, runbookMenu(row, rb))"
                >
                  <span class="component">{{ row.component.name }}</span>
                  <span class="title">{{ rb.title }}</span>
                  <span class="muted nowrap">{{
                    'runbooks.summary' | t: { steps: rb.stepCount, minutes: rb.criticalPathMinutes }
                  }}</span>
                  @if (rb.exceedsRto) {
                    <span class="status error" [matTooltip]="'runbooks.exceedsRtoHint' | t">{{
                      'runbooks.exceedsRto' | t
                    }}</span>
                  }
                  @for (i of runbookIssues(rb); track i.ruleId) {
                    <mat-icon
                      class="issue"
                      [class.blocking]="i.severity === 'blocking'"
                      [matTooltip]="i.message"
                      [attr.aria-label]="i.message"
                      >error</mat-icon
                    >
                  }
                  <span class="flex-grow-1"></span>
                  <button
                    mat-icon-button
                    (click)="menu.open($event, runbookMenu(row, rb))"
                    [attr.aria-label]="'common.actions' | t"
                  >
                    <mat-icon>more_vert</mat-icon>
                  </button>
                </li>
              } @empty {
                <li
                  class="row-item missing"
                  tabindex="0"
                  (contextmenu)="menu.open($event, missingMenu(row))"
                >
                  <span class="component">{{ row.component.name }}</span>
                  <span class="title">{{ 'runbooks.missing' | t }}</span>
                  <span class="flex-grow-1"></span>
                  <button mat-stroked-button (click)="create(row)" [disabled]="m.busy()">
                    <mat-icon>add</mat-icon>{{ 'runbooks.create' | t }}
                  </button>
                </li>
              }
            }
          </ul>
        </section>
      } @empty {
        @if (data()) {
          <p class="muted">{{ 'runbooks.noScenarios' | t }}</p>
        }
      }
    }
    <app-context-menu #menu />
  `,
  styles: `
    .section-title {
      font: var(--mat-sys-title-medium);
      margin: 24px 0 4px;
    }
    .small {
      font: var(--mat-sys-body-small);
    }
    .group-title {
      font: var(--mat-sys-title-small);
      margin: 16px 0 4px;
    }
    .rows {
      list-style: none;
      margin: 0;
      padding: 0;
    }
    .row-item {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 8px;
      padding: 4px 8px;
      border-bottom: 1px solid var(--mat-sys-outline-variant);
      cursor: pointer;
    }
    .row-item:hover,
    .row-item:focus-visible {
      background: var(--mat-sys-surface-container-low);
    }
    .row-item.missing {
      cursor: default;
      background: var(--mat-sys-error-container);
      color: var(--mat-sys-on-error-container);
    }
    .component {
      font: var(--mat-sys-label-large);
      min-width: 120px;
    }
    .nowrap {
      white-space: nowrap;
    }
    .issue {
      color: var(--mat-sys-tertiary);
    }
    .issue.blocking {
      color: var(--mat-sys-error);
    }
  `,
})
export class RunbooksView {
  readonly serviceId = input.required<string>();
  readonly reloadKey = input(0);
  readonly changed = output<void>();

  private readonly api = inject(Api);
  private readonly i18n = inject(I18n);
  private readonly dialog = inject(MatDialog);
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);
  protected readonly m = injectMutation();

  protected readonly openId = toSignal(
    this.route.queryParamMap.pipe(map((p) => p.get('runbook'))),
    { initialValue: null },
  );

  protected readonly resource = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey() }),
    stream: ({ params }) =>
      forkJoin({
        runbooks: this.api.serviceRunbooks(params.id),
        components: this.api.microservices(params.id),
        scenarios: this.api.scenarios(params.id),
        strategies: this.api.serviceStrategies(params.id),
        issues: this.api
          .workflow(params.id)
          .pipe(map((w) => w.steps.filter((s) => s.key === 'runbooks').flatMap((s) => s.issues))),
      }),
  });
  /** Keeps the previous data while reloading (no flicker). */
  protected readonly data = linkedSignal<RunbooksData | undefined, RunbooksData | undefined>({
    source: () => this.resource.value(),
    computation: (value, previous) => value ?? previous?.value,
  });
  protected readonly groups = computed(() => {
    const d = this.data();
    return d ? runbookGroups(d.scenarios, d.components, d.runbooks) : [];
  });

  protected runbookIssues(rb: Runbook): Issue[] {
    return issuesOf(this.data()?.issues ?? [], 'runbook', rb.id);
  }

  protected open(id: string | null): void {
    void this.router.navigate([], {
      relativeTo: this.route,
      queryParams: { runbook: id },
      queryParamsHandling: 'merge',
    });
  }

  protected runbookMenu(row: RunbookRow, rb: Runbook): MenuItem[] {
    return [
      { icon: 'edit_note', label: 'runbooks.openSteps', run: () => this.open(rb.id) },
      { icon: 'edit', label: 'runbooks.edit', run: () => this.edit(row, rb) },
      { icon: 'add', label: 'runbooks.createAnother', run: () => this.create(row) },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        run: () => this.remove(rb),
      },
    ];
  }

  protected missingMenu(row: RunbookRow): MenuItem[] {
    return [{ icon: 'add', label: 'runbooks.create', run: () => this.create(row) }];
  }

  protected create(row: RunbookRow): void {
    const request = openEdit(this.dialog, {
      title: 'runbooks.create',
      submit: 'common.create',
      fields: this.fields(row),
      value: { title: '', strategyId: this.defaultStrategy(row) },
    }).pipe(
      filter((v): v is EditResult => !!v),
      switchMap((v) =>
        this.api.createRunbook(row.component.id, {
          scenarioId: row.scenario.id,
          title: String(v['title']).trim(),
          description: String(v['description'] ?? ''),
          strategyId: (v['strategyId'] as string | null) ?? null,
        }),
      ),
    );
    this.save(request, (rb) => this.open(rb.id));
  }

  protected edit(row: RunbookRow, rb: Runbook): void {
    const request = openEdit(this.dialog, {
      title: 'runbooks.edit',
      fields: this.fields(row),
      value: {
        title: rb.title,
        description: rb.description ?? '',
        strategyId: rb.strategyId ?? null,
      },
    }).pipe(
      filter((v): v is EditResult => !!v),
      switchMap((v) =>
        this.api.updateRunbook(rb.id, {
          title: String(v['title']).trim(),
          description: String(v['description'] ?? ''),
          strategyId: (v['strategyId'] as string | null) ?? null,
        }),
      ),
    );
    this.save(request);
  }

  private remove(rb: Runbook): void {
    const request = openConfirm(this.dialog, {
      title: 'runbooks.deleteConfirm',
      message: rb.title,
      confirm: 'common.delete',
    }).pipe(
      filter((ok) => !!ok),
      switchMap(() => this.api.deleteRunbook(rb.id)),
    );
    this.save(request, undefined, 'common.deleted');
  }

  private fields(row: RunbookRow): FieldDef[] {
    const strategies = (this.data()?.strategies ?? []).filter(
      (s) => s.microserviceId === row.component.id && s.scenarioIds.includes(row.scenario.id),
    );
    return [
      { key: 'title', label: 'runbooks.runbookTitle', type: 'text', required: true },
      { key: 'description', label: 'common.description', type: 'textarea' },
      {
        key: 'strategyId',
        label: 'runbooks.strategy',
        type: 'select',
        hint: this.i18n.t('runbooks.strategyHint'),
        options: [
          { value: null, label: '–' },
          ...strategies.map((s) => ({ value: s.id, label: s.title ?? s.type })),
        ],
      },
    ];
  }

  /** The selected measure for this scenario and component, if any. */
  private defaultStrategy(row: RunbookRow): string | null {
    return (
      (this.data()?.strategies ?? []).find(
        (s) =>
          s.isSelected &&
          s.microserviceId === row.component.id &&
          s.scenarioIds.includes(row.scenario.id),
      )?.id ?? null
    );
  }

  private save<T>(
    request: Observable<T>,
    done?: (value: T) => void,
    success = 'common.saved',
  ): void {
    this.m.run(request, {
      success,
      done: (value) => {
        done?.(value);
        this.changed.emit();
      },
    });
  }
}
