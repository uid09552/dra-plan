import { Component, computed, input, output, signal } from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';
import { MatTooltipModule } from '@angular/material/tooltip';

import { Scenario } from '../../../core/api/models';
import { TranslatePipe } from '../../../core/i18n/i18n';

const LEVELS = [1, 2, 3, 4] as const;

/** Risk band of likelihood × impact (1–16), as used for the scenario priority. */
export function riskBand(score: number): 'low' | 'medium' | 'high' | 'critical' {
  if (score >= 12) {
    return 'critical';
  }
  if (score >= 8) {
    return 'high';
  }
  if (score >= 4) {
    return 'medium';
  }
  return 'low';
}

/** 4×4 risk matrix: likelihood (rows, high on top) × impact (columns). */
@Component({
  selector: 'app-risk-matrix',
  imports: [NgTemplateOutlet, MatTooltipModule, TranslatePipe],
  template: `
    <div class="matrix" role="table" [attr.aria-label]="'risk.title' | t">
      <div class="axis-y" aria-hidden="true">{{ 'risk.likelihood' | t }} →</div>
      <div class="grid">
        @for (l of rowsTopDown; track l) {
          <div class="row-label" role="rowheader">{{ l }}</div>
          @for (i of levels; track i) {
            <div
              class="cell"
              [class]="band(l * i)"
              [class.over]="over() === l + '|' + i"
              role="cell"
              (dragover)="allowDrop($event, l, i)"
              (dragleave)="over.set(null)"
              (drop)="drop($event, l, i)"
            >
              <span class="score">{{ l * i }}</span>
              @for (s of cell(l, i); track s.id) {
                <ng-container *ngTemplateOutlet="chip; context: { $implicit: s }" />
              }
            </div>
          }
        }
        <div></div>
        @for (i of levels; track i) {
          <div class="col-label">{{ i }}</div>
        }
      </div>
      <div class="axis-x muted">{{ 'risk.impact' | t }} →</div>
    </div>
    @if (unrated().length) {
      <div class="tray mt-2">
        <span class="muted">{{ 'risk.unrated' | t: { count: unrated().length } }}:</span>
        @for (s of unrated(); track s.id) {
          <ng-container *ngTemplateOutlet="chip; context: { $implicit: s }" />
        }
      </div>
    }
    <p class="muted small mt-2">{{ 'risk.dragHint' | t }}</p>

    <ng-template #chip let-s>
      <button
        type="button"
        class="chip"
        draggable="true"
        [class.rejected]="s.status === 'rejected'"
        [class.selected]="s.status === 'selected'"
        [matTooltip]="s.title"
        (click)="picked.emit(s)"
        (dragstart)="dragStart($event, s)"
        (contextmenu)="menuRequested.emit({ event: $event, scenario: s })"
      >
        {{ s.title }}
      </button>
    </ng-template>
  `,
  styles: `
    .matrix {
      display: grid;
      grid-template-columns: 24px 1fr;
      grid-template-rows: 1fr auto;
      gap: 4px;
      max-width: 900px;
    }
    .axis-y {
      writing-mode: vertical-rl;
      transform: rotate(180deg);
      text-align: center;
      color: var(--mat-sys-on-surface-variant);
      font: var(--mat-sys-label-medium);
    }
    .axis-x {
      grid-column: 2;
      text-align: center;
      font: var(--mat-sys-label-medium);
    }
    .grid {
      display: grid;
      grid-template-columns: 20px repeat(4, minmax(0, 1fr));
      gap: 4px;
    }
    .row-label,
    .col-label {
      display: flex;
      align-items: center;
      justify-content: center;
      font: var(--mat-sys-label-medium);
      color: var(--mat-sys-on-surface-variant);
    }
    .cell {
      min-height: 72px;
      border-radius: 8px;
      padding: 4px;
      display: flex;
      flex-wrap: wrap;
      align-content: flex-start;
      gap: 4px;
      position: relative;
    }
    .cell.low {
      background: light-dark(#e8f5e9, #1b3a1e);
    }
    .cell.medium {
      background: light-dark(#fff8e1, #3d3514);
    }
    .cell.high {
      background: light-dark(#ffe0b2, #4a2c0c);
    }
    .cell.critical {
      background: light-dark(#ffcdd2, #4d1a1a);
    }
    .score {
      position: absolute;
      right: 6px;
      bottom: 2px;
      font: var(--mat-sys-label-small);
      opacity: 0.6;
    }
    .chip {
      max-width: 100%;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      border: 1px solid var(--mat-sys-outline);
      background: var(--mat-sys-surface);
      color: var(--mat-sys-on-surface);
      border-radius: 12px;
      padding: 2px 8px;
      font: var(--mat-sys-label-small);
      cursor: pointer;
    }
    .chip {
      cursor: grab;
    }
    .chip.selected {
      border-color: light-dark(#2e7d32, #81c784);
      border-width: 2px;
    }
    .cell.over {
      outline: 3px dashed var(--mat-sys-primary);
      outline-offset: -3px;
    }
    .tray {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      align-items: center;
      max-width: 900px;
      padding: 8px;
      border: 1px dashed var(--mat-sys-outline-variant);
      border-radius: 8px;
    }
    .small {
      font: var(--mat-sys-body-small);
    }
    .chip.rejected {
      opacity: 0.55;
      text-decoration: line-through;
    }
  `,
})
export class RiskMatrix {
  readonly scenarios = input.required<Scenario[]>();
  readonly picked = output<Scenario>();
  /** Scenario dropped on a cell. */
  readonly rated = output<{ scenario: Scenario; likelihood: number; impact: number }>();
  readonly menuRequested = output<{ event: MouseEvent; scenario: Scenario }>();

  /** Cell under the dragged chip (`likelihood|impact`). */
  protected readonly over = signal<string | null>(null);

  protected readonly levels = LEVELS;
  protected readonly rowsTopDown = [...LEVELS].reverse();

  private readonly ratedScenarios = computed(() =>
    this.scenarios().filter((s) => s.status !== 'merged' && s.likelihood && s.impact),
  );
  protected readonly unrated = computed(() =>
    this.scenarios().filter((s) => s.status !== 'merged' && (!s.likelihood || !s.impact)),
  );

  protected cell(likelihood: number, impact: number): Scenario[] {
    return this.ratedScenarios().filter((s) => s.likelihood === likelihood && s.impact === impact);
  }

  protected dragStart(event: DragEvent, s: Scenario): void {
    event.dataTransfer?.setData('text/plain', s.id);
    if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = 'move';
    }
  }

  protected allowDrop(event: DragEvent, likelihood: number, impact: number): void {
    event.preventDefault();
    this.over.set(`${likelihood}|${impact}`);
  }

  protected drop(event: DragEvent, likelihood: number, impact: number): void {
    event.preventDefault();
    this.over.set(null);
    const id = event.dataTransfer?.getData('text/plain');
    const scenario = this.scenarios().find((s) => s.id === id);
    if (scenario) {
      this.rated.emit({ scenario, likelihood, impact });
    }
  }

  protected band(score: number): string {
    return riskBand(score);
  }
}
