import { Component, effect, inject, signal } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { FormBuilder, ReactiveFormsModule, Validators } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatButtonToggleModule } from '@angular/material/button-toggle';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatSlideToggleModule } from '@angular/material/slide-toggle';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatTableModule } from '@angular/material/table';
import { MatTabsModule } from '@angular/material/tabs';
import { MatTooltipModule } from '@angular/material/tooltip';

import { Api } from '../../core/api/api';
import { issuesOf } from '../../core/http/interceptors';
import { I18n, Lang, TranslatePipe } from '../../core/i18n/i18n';
import { Theme, ThemeMode } from '../../core/theme/theme';

@Component({
  selector: 'app-settings',
  imports: [
    ReactiveFormsModule,
    MatCardModule,
    MatButtonModule,
    MatButtonToggleModule,
    MatFormFieldModule,
    MatInputModule,
    MatSlideToggleModule,
    MatIconModule,
    MatTableModule,
    MatTabsModule,
    MatTooltipModule,
    TranslatePipe,
  ],
  template: `
    <div class="page">
      <h1 class="page-title">{{ 'settings.title' | t }}</h1>
      <p class="page-subtitle">{{ 'settings.subtitle' | t }}</p>
      <mat-tab-group>
        <mat-tab [label]="'settings.general' | t">
          <div class="row g-3 pt-3">
            <div class="col-12 col-lg-6">
              <mat-card appearance="outlined" class="h-100">
                <mat-card-header
                  ><mat-card-title>{{ 'settings.personal' | t }}</mat-card-title></mat-card-header
                >
                <mat-card-content class="d-flex flex-column gap-3 pt-3">
                  <div>
                    <div class="label">{{ 'lang.label' | t }}</div>
                    <mat-button-toggle-group
                      [value]="i18n.lang()"
                      (change)="setLang($event.value)"
                      [attr.aria-label]="'lang.label' | t"
                    >
                      <mat-button-toggle value="en">{{ 'lang.en' | t }}</mat-button-toggle>
                      <mat-button-toggle value="de">{{ 'lang.de' | t }}</mat-button-toggle>
                    </mat-button-toggle-group>
                  </div>
                  <div>
                    <div class="label">{{ 'theme.label' | t }}</div>
                    <mat-button-toggle-group
                      [value]="theme.mode()"
                      (change)="setTheme($event.value)"
                      [attr.aria-label]="'theme.label' | t"
                    >
                      <mat-button-toggle value="system"
                        ><mat-icon>brightness_auto</mat-icon>
                        {{ 'theme.system' | t }}</mat-button-toggle
                      >
                      <mat-button-toggle value="light"
                        ><mat-icon>light_mode</mat-icon> {{ 'theme.light' | t }}</mat-button-toggle
                      >
                      <mat-button-toggle value="dark"
                        ><mat-icon>dark_mode</mat-icon> {{ 'theme.dark' | t }}</mat-button-toggle
                      >
                    </mat-button-toggle-group>
                  </div>
                </mat-card-content>
              </mat-card>
            </div>
            <div class="col-12 col-lg-6">
              <mat-card appearance="outlined" class="h-100">
                <mat-card-header>
                  <mat-card-title>{{ 'settings.tenant' | t }}</mat-card-title>
                  <mat-card-subtitle>{{ tenant.value()?.name }}</mat-card-subtitle>
                </mat-card-header>
                <form [formGroup]="form" (ngSubmit)="save()">
                  <mat-card-content class="d-flex flex-column pt-3">
                    <mat-form-field>
                      <mat-label>{{ 'settings.reviewInterval' | t }}</mat-label>
                      <input
                        matInput
                        type="number"
                        min="1"
                        max="120"
                        formControlName="reviewIntervalMonths"
                      />
                    </mat-form-field>
                    <mat-form-field>
                      <mat-label>{{ 'settings.tolerance' | t }}</mat-label>
                      <input
                        matInput
                        type="number"
                        min="1"
                        max="4"
                        formControlName="impactToleranceLevel"
                      />
                    </mat-form-field>
                    <mat-slide-toggle formControlName="aiEnabled">{{
                      'settings.aiEnabled' | t
                    }}</mat-slide-toggle>
                    @for (e of errors(); track e) {
                      <div class="status error mt-2">{{ e }}</div>
                    }
                  </mat-card-content>
                  <mat-card-actions align="end">
                    <button
                      mat-flat-button
                      type="submit"
                      [disabled]="form.invalid || form.pristine"
                    >
                      {{ 'common.save' | t }}
                    </button>
                  </mat-card-actions>
                </form>
              </mat-card>
            </div>
          </div>
        </mat-tab>
        <mat-tab [label]="'settings.categories.title' | t">
          <div class="pt-3">
            <mat-card appearance="outlined">
              <mat-card-content class="pt-3">
                <p class="page-subtitle">{{ 'settings.categories.subtitle' | t }}</p>
                <table mat-table [dataSource]="categories.value() ?? []" class="w-100">
                  <ng-container matColumnDef="label">
                    <th mat-header-cell *matHeaderCellDef>{{ 'settings.categories.label' | t }}</th>
                    <td mat-cell *matCellDef="let c">{{ c.label }}</td>
                  </ng-container>
                  <ng-container matColumnDef="key">
                    <th mat-header-cell *matHeaderCellDef>{{ 'settings.categories.key' | t }}</th>
                    <td mat-cell *matCellDef="let c">
                      <code>{{ c.key }}</code>
                    </td>
                  </ng-container>
                  <ng-container matColumnDef="actions">
                    <th mat-header-cell *matHeaderCellDef></th>
                    <td mat-cell *matCellDef="let c">
                      <button
                        mat-icon-button
                        [attr.aria-label]="'settings.categories.delete' | t"
                        [matTooltip]="'settings.categories.delete' | t"
                        (click)="deleteCategory(c.id)"
                      >
                        <mat-icon>delete</mat-icon>
                      </button>
                    </td>
                  </ng-container>
                  <tr mat-header-row *matHeaderRowDef="categoryColumns"></tr>
                  <tr mat-row *matRowDef="let row; columns: categoryColumns"></tr>
                </table>
                @if (!categories.value()?.length) {
                  <p class="muted">{{ 'settings.categories.empty' | t }}</p>
                }
              </mat-card-content>
              <form [formGroup]="categoryForm" (ngSubmit)="addCategory()">
                <mat-card-content class="d-flex flex-wrap align-items-start gap-3">
                  <mat-form-field>
                    <mat-label>{{ 'settings.categories.label' | t }}</mat-label>
                    <input matInput formControlName="label" />
                  </mat-form-field>
                  <mat-form-field>
                    <mat-label>{{ 'settings.categories.key' | t }}</mat-label>
                    <input matInput formControlName="key" />
                    <mat-hint>{{ 'settings.categories.keyHint' | t }}</mat-hint>
                  </mat-form-field>
                  @for (e of categoryErrors(); track e) {
                    <div class="status error">{{ e }}</div>
                  }
                </mat-card-content>
                <mat-card-actions align="end">
                  <button mat-flat-button type="submit" [disabled]="categoryForm.invalid">
                    <mat-icon>add</mat-icon> {{ 'settings.categories.add' | t }}
                  </button>
                </mat-card-actions>
              </form>
            </mat-card>
          </div>
        </mat-tab>
      </mat-tab-group>
    </div>
  `,
  styles: `
    .label {
      font: var(--mat-sys-label-large);
      margin-bottom: 8px;
    }
  `,
})
export class Settings {
  protected readonly i18n = inject(I18n);
  protected readonly theme = inject(Theme);
  private readonly api = inject(Api);
  private readonly snack = inject(MatSnackBar);
  protected readonly tenant = rxResource({ stream: () => this.api.tenant() });
  protected readonly errors = signal<string[]>([]);
  protected readonly categories = rxResource({ stream: () => this.api.categories() });
  protected readonly categoryErrors = signal<string[]>([]);
  protected readonly categoryColumns = ['label', 'key', 'actions'];

  protected readonly form = inject(FormBuilder).nonNullable.group({
    reviewIntervalMonths: [12, [Validators.required, Validators.min(1), Validators.max(120)]],
    impactToleranceLevel: [3, [Validators.required, Validators.min(1), Validators.max(4)]],
    aiEnabled: [true],
  });

  protected readonly categoryForm = inject(FormBuilder).nonNullable.group({
    key: ['', [Validators.required, Validators.pattern(/^[a-z0-9_-]{2,40}$/)]],
    label: ['', [Validators.required, Validators.maxLength(80)]],
  });

  constructor() {
    effect(() => {
      const s = this.tenant.value()?.settings;
      if (s) {
        this.form.reset({
          reviewIntervalMonths: s.reviewIntervalMonths,
          impactToleranceLevel: s.impactToleranceLevel,
          aiEnabled: s.aiEnabled,
        });
      }
    });
  }

  protected setLang(lang: Lang): void {
    this.i18n.use(lang);
  }

  protected setTheme(mode: ThemeMode): void {
    this.theme.use(mode);
  }

  protected save(): void {
    this.api.updateTenantSettings(this.form.getRawValue()).subscribe({
      next: () => {
        this.errors.set([]);
        this.snack.open(this.i18n.t('common.saved'), undefined, { duration: 3000 });
        this.tenant.reload();
      },
      error: (e: unknown) => this.errors.set(issuesOf(e)),
    });
  }

  protected addCategory(): void {
    this.api.createCategory(this.categoryForm.getRawValue()).subscribe({
      next: () => {
        this.categoryErrors.set([]);
        this.categoryForm.reset({ key: '', label: '' });
        this.categories.reload();
      },
      error: (e: unknown) => this.categoryErrors.set(issuesOf(e)),
    });
  }

  protected deleteCategory(id: string): void {
    this.api.deleteCategory(id).subscribe({
      next: () => this.categories.reload(),
      error: (e: unknown) => this.categoryErrors.set(issuesOf(e)),
    });
  }
}
