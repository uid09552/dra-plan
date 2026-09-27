import { DatePipe } from '@angular/common';
import { Component, computed, inject, input, linkedSignal, output, signal } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSelectModule } from '@angular/material/select';
import { MatTooltipModule } from '@angular/material/tooltip';
import { forkJoin, Observable } from 'rxjs';

import { Api } from '../../../core/api/api';
import { Microservice, RecoveryStrategy, Scenario } from '../../../core/api/models';
import { injectMutation } from '../../../core/http/mutation';
import { I18n, TranslatePipe } from '../../../core/i18n/i18n';
import { ContextMenu, MenuItem } from '../../../core/ui/context-menu';
import { MeasureContext, MeasureEditor } from './measure-editor';
import { affectedComponents } from './affected-components';

interface MeasuresData {
  strategies: RecoveryStrategy[];
  components: Microservice[];
  scenarios: Scenario[];
}

/** A selected DR scenario × affected component without a selected measure. */
interface Uncovered {
  scenario: Scenario;
  componentId: string;
  componentName: string;
}

/** Measures (recovery strategies) of a service: each covers one or more scenarios. */
@Component({
  selector: 'app-measures-view',
  imports: [
    DatePipe,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatProgressBarModule,
    MatSelectModule,
    MatTooltipModule,
    TranslatePipe,
    ContextMenu,
  ],
  template: `
    <div class="d-flex flex-wrap align-items-center gap-2 mb-2">
      <mat-form-field class="filter" subscriptSizing="dynamic">
        <mat-label>{{ 'measures.filter' | t }}</mat-label>
        <mat-select [value]="scenarioFilter()" (selectionChange)="scenarioFilter.set($event.value)">
          <mat-option [value]="null">{{ 'measures.allScenarios' | t }}</mat-option>
          @for (s of activeScenarios(); track s.id) {
            <mat-option [value]="s.id">{{ s.title }}</mat-option>
          }
        </mat-select>
      </mat-form-field>
      <span class="flex-grow-1"></span>
      <button mat-flat-button (click)="add()" [disabled]="m.busy() || !data()">
        <mat-icon>add</mat-icon>{{ 'measures.add' | t }}
      </button>
    </div>
    <p class="muted small">{{ 'measures.hint' | t }}</p>
    @for (e of m.issues(); track e) {
      <div class="status error mb-2">{{ e }}</div>
    }
    @if (resource.isLoading() && !data()) {
      <mat-progress-bar mode="indeterminate" />
    }

    @if (uncovered().length) {
      <div class="uncovered mb-3">
        <strong>{{ 'measures.uncovered' | t }}:</strong>
        @for (u of uncovered(); track u.scenario.id + u.componentId) {
          <button
            mat-stroked-button
            class="gap-chip"
            (click)="add([u.scenario.id], u.componentId)"
            [matTooltip]="'measures.addFor' | t"
          >
            <mat-icon>add</mat-icon>{{ u.scenario.title }} · {{ u.componentName }}
          </button>
        }
      </div>
    }

    <div class="table-scroll">
      <table class="grid-table">
        <thead>
          <tr>
            <th>{{ 'measures.measure' | t }}</th>
            <th>{{ 'guide.components.component' | t }}</th>
            <th>{{ 'measures.scenarios' | t }}</th>
            <th>RTO / RPO</th>
            <th>{{ 'guide.mitigations.implementation' | t }}</th>
            <th>{{ 'guide.mitigations.lastTested' | t }}</th>
            <th>{{ 'measures.inPlan' | t }}</th>
            <th class="actions"></th>
          </tr>
        </thead>
        <tbody>
          @for (st of visible(); track st.id) {
            <tr (contextmenu)="menu.open($event, rowMenu(st))" (dblclick)="edit(st)">
              <td>{{ label(st) }}</td>
              <td>{{ componentName(st.microserviceId) }}</td>
              <td>
                <div class="d-flex flex-wrap gap-1">
                  @for (id of st.scenarioIds; track id) {
                    <span class="status info">{{ scenarioTitle(id) }}</span>
                  }
                </div>
              </td>
              <td class="nowrap">
                {{ st.estimatedRtoMinutes }} / {{ st.estimatedRpoMinutes }} min
                @if (st.gapCheck?.status === 'gap') {
                  <span class="status error" [matTooltip]="'measures.gapTooltip' | t">{{
                    'guide.mitigations.gap' | t
                  }}</span>
                }
              </td>
              <td>
                <span
                  class="status"
                  [class]="st.implementationStatus === 'implemented' ? 'ok' : 'warn'"
                  >{{ 'guide.mitigations.status.' + st.implementationStatus | t }}</span
                >
              </td>
              <td class="nowrap">
                {{ st.lastTestedAt ? (st.lastTestedAt | date) : ('guide.mitigations.never' | t) }}
              </td>
              <td>
                @if (st.isSelected) {
                  <mat-icon class="ok" [matTooltip]="'guide.mitigations.selectedHint' | t"
                    >check_circle</mat-icon
                  >
                } @else {
                  <button mat-button (click)="select(st)" [disabled]="m.busy()">
                    {{ 'guide.mitigations.select' | t }}
                  </button>
                }
              </td>
              <td class="actions">
                <button
                  mat-icon-button
                  (click)="menu.open($event, rowMenu(st))"
                  [attr.aria-label]="'common.actions' | t"
                >
                  <mat-icon>more_vert</mat-icon>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="8" class="muted">{{ 'measures.empty' | t }}</td>
            </tr>
          }
        </tbody>
      </table>
    </div>
    <p class="muted small mt-2">{{ 'guide.mitigations.runbookHint' | t }}</p>

    <app-context-menu #menu />
  `,
  styles: `
    .filter {
      min-width: 260px;
    }
    .small {
      font: var(--mat-sys-body-small);
    }
    .uncovered {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      align-items: center;
      padding: 8px 12px;
      border-radius: 8px;
      background: var(--mat-sys-error-container);
      color: var(--mat-sys-on-error-container);
    }
    .gap-chip {
      --mat-button-outlined-label-text-color: var(--mat-sys-on-error-container);
    }
    .grid-table {
      border-collapse: collapse;
      width: 100%;
    }
    .grid-table th,
    .grid-table td {
      border-bottom: 1px solid var(--mat-sys-outline-variant);
      padding: 6px 8px;
      text-align: left;
      vertical-align: middle;
    }
    .grid-table thead th {
      font: var(--mat-sys-label-large);
      color: var(--mat-sys-on-surface-variant);
    }
    tbody tr:hover {
      background: var(--mat-sys-surface-container-low);
    }
    .actions {
      width: 1%;
      white-space: nowrap;
      text-align: right;
    }
    .nowrap {
      white-space: nowrap;
    }
    .ok {
      color: light-dark(#2e7d32, #81c784);
    }
  `,
})
export class MeasuresView {
  readonly serviceId = input.required<string>();
  readonly reloadKey = input(0);
  readonly changed = output<void>();

  private readonly api = inject(Api);
  private readonly i18n = inject(I18n);
  private readonly editor = inject(MeasureEditor);
  protected readonly m = injectMutation();

  protected readonly resource = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey() }),
    stream: ({ params }) =>
      forkJoin({
        strategies: this.api.serviceStrategies(params.id),
        components: this.api.microservices(params.id),
        scenarios: this.api.scenarios(params.id),
      }),
  });
  /** Keeps the previous data while reloading (no flicker). */
  protected readonly data = linkedSignal<MeasuresData | undefined, MeasuresData | undefined>({
    source: () => this.resource.value(),
    computation: (value, previous) => value ?? previous?.value,
  });
  private readonly catalog = rxResource({
    params: () => this.i18n.lang(),
    stream: () => this.api.catalog(),
  });

  protected readonly scenarioFilter = signal<string | null>(null);
  protected readonly activeScenarios = computed(() =>
    (this.data()?.scenarios ?? []).filter((s) => s.status !== 'merged'),
  );
  protected readonly visible = computed(() => {
    const filter = this.scenarioFilter();
    const all = this.data()?.strategies ?? [];
    return filter ? all.filter((s) => s.scenarioIds.includes(filter)) : all;
  });
  protected readonly uncovered = computed<Uncovered[]>(() => {
    const d = this.data();
    if (!d) {
      return [];
    }
    const drScenarios = d.scenarios.filter(
      (s) =>
        s.status === 'selected' && (s.drRequired === 'yes' || s.drRequired === 'degraded_mode'),
    );
    return drScenarios.flatMap((scenario) =>
      affectedComponents(scenario, d.components)
        .filter(
          (c) =>
            !d.strategies.some(
              (st) =>
                st.isSelected && st.microserviceId === c.id && st.scenarioIds.includes(scenario.id),
            ),
        )
        .map((c) => ({ scenario, componentId: c.id, componentName: c.name })),
    );
  });

  protected label(st: RecoveryStrategy): string {
    return this.editor.label(st, this.catalog.value());
  }

  protected componentName(id: string): string {
    return this.data()?.components.find((c) => c.id === id)?.name ?? '–';
  }

  protected scenarioTitle(id: string): string {
    return this.data()?.scenarios.find((s) => s.id === id)?.title ?? '–';
  }

  protected rowMenu(st: RecoveryStrategy): MenuItem[] {
    return [
      { icon: 'edit', label: 'common.edit', run: () => this.edit(st) },
      {
        icon: 'check_circle',
        label: 'guide.mitigations.select',
        disabled: st.isSelected,
        run: () => this.select(st),
      },
      { icon: 'add', label: 'measures.add', run: () => this.add() },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        run: () => this.save(this.editor.remove(st, this.label(st)), 'common.deleted'),
      },
    ];
  }

  protected add(scenarioIds?: string[], microserviceId?: string): void {
    this.save(this.editor.add(this.context(), { scenarioIds, microserviceId }));
  }

  protected edit(st: RecoveryStrategy): void {
    this.save(this.editor.edit(st, this.context()));
  }

  protected select(st: RecoveryStrategy): void {
    this.save(this.editor.select(st), 'guide.mitigations.selected');
  }

  private context(): MeasureContext {
    const d = this.data();
    return {
      components: d?.components ?? [],
      scenarios: d?.scenarios ?? [],
      catalog: this.catalog.value(),
    };
  }

  private save(request: Observable<unknown>, success = 'common.saved'): void {
    this.m.run(request, { success, done: () => this.changed.emit() });
  }
}
