import { Component, computed, inject } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatCardModule } from '@angular/material/card';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatTableModule } from '@angular/material/table';
import { MatTabsModule } from '@angular/material/tabs';

import { Api } from '../../core/api/api';
import { I18n, TranslatePipe } from '../../core/i18n/i18n';

/** Knowledge base: scenario templates, strategy types and the NIST/BSI-mapped workflow. */
@Component({
  selector: 'app-catalog',
  imports: [MatTabsModule, MatTableModule, MatCardModule, MatProgressBarModule, TranslatePipe],
  template: `
    <div class="page">
      <h1 class="page-title">{{ 'catalog.title' | t }}</h1>
      <p class="page-subtitle">{{ 'catalog.subtitle' | t }}</p>
      @if (catalog.isLoading()) {
        <mat-progress-bar mode="indeterminate" />
      }
      @if (catalog.value(); as c) {
        <mat-tab-group mat-stretch-tabs="false" animationDuration="0ms">
          <mat-tab [label]="'catalog.steps' | t">
            <div class="table-scroll py-3">
              <table mat-table [dataSource]="c.workflowSteps" class="full-width">
                <ng-container matColumnDef="number">
                  <th mat-header-cell *matHeaderCellDef>#</th>
                  <td mat-cell *matCellDef="let s">{{ s.number }}</td>
                </ng-container>
                <ng-container matColumnDef="title">
                  <th mat-header-cell *matHeaderCellDef>{{ 'common.name' | t }}</th>
                  <td mat-cell *matCellDef="let s">{{ s.title }}</td>
                </ng-container>
                <ng-container matColumnDef="nist">
                  <th mat-header-cell *matHeaderCellDef>NIST SP 800-34</th>
                  <td mat-cell *matCellDef="let s" class="muted">{{ s.nistRef }}</td>
                </ng-container>
                <ng-container matColumnDef="bsi">
                  <th mat-header-cell *matHeaderCellDef>BSI 200-4</th>
                  <td mat-cell *matCellDef="let s" class="muted">{{ s.bsiRef }}</td>
                </ng-container>
                <tr mat-header-row *matHeaderRowDef="stepColumns"></tr>
                <tr mat-row *matRowDef="let row; columns: stepColumns"></tr>
              </table>
            </div>
          </mat-tab>
          <mat-tab [label]="'catalog.strategies' | t">
            <div class="row g-3 py-3">
              @for (s of c.strategyTypes; track s.key) {
                <div class="col-12 col-md-6 col-xl-4">
                  <mat-card appearance="outlined" class="h-100">
                    <mat-card-header>
                      <mat-card-title>{{ s.label }}</mat-card-title>
                      <mat-card-subtitle>
                        {{ 'catalog.typicalRto' | t }}: {{ s.typicalRto }} ·
                        {{ 'catalog.typicalRpo' | t }}: {{ s.typicalRpo }}
                      </mat-card-subtitle>
                    </mat-card-header>
                    <mat-card-content class="pt-2">{{ s.notes }}</mat-card-content>
                  </mat-card>
                </div>
              }
            </div>
          </mat-tab>
          <mat-tab [label]="'catalog.templates' | t">
            <div class="row g-3 py-3">
              @for (t of c.scenarioTemplates; track t.id) {
                <div class="col-12 col-md-6 col-xl-4">
                  <mat-card appearance="outlined" class="h-100">
                    <mat-card-header>
                      <mat-card-title>{{ t.title }}</mat-card-title>
                      <mat-card-subtitle>{{
                        categoryLabel()[t.category] ?? t.category
                      }}</mat-card-subtitle>
                    </mat-card-header>
                    <mat-card-content class="pt-2">{{ t.description }}</mat-card-content>
                  </mat-card>
                </div>
              }
            </div>
          </mat-tab>
        </mat-tab-group>
      }
    </div>
  `,
})
export class CatalogPage {
  private readonly api = inject(Api);
  private readonly i18n = inject(I18n);
  /** Reloaded when the language changes (labels come localized from the backend). */
  protected readonly catalog = rxResource({
    params: () => this.i18n.lang(),
    stream: () => this.api.catalog(),
  });
  protected readonly stepColumns = ['number', 'title', 'nist', 'bsi'];
  protected readonly categoryLabel = computed<Partial<Record<string, string>>>(() =>
    Object.fromEntries(
      (this.catalog.value()?.scenarioCategories ?? []).map((c) => [c.key, c.label]),
    ),
  );
}
