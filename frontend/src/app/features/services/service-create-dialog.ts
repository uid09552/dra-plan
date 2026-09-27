import { Component, inject, signal } from '@angular/core';
import { FormBuilder, ReactiveFormsModule, Validators } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatDialogModule, MatDialogRef } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';
import { MatSelectModule } from '@angular/material/select';

import { Api } from '../../core/api/api';
import { ImpactLevel, ItService, ProtectionRequirement } from '../../core/api/models';
import { issuesOf } from '../../core/http/interceptors';
import { TranslatePipe } from '../../core/i18n/i18n';

@Component({
  selector: 'app-service-create-dialog',
  imports: [
    ReactiveFormsModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
    MatButtonModule,
    TranslatePipe,
  ],
  template: `
    <h2 mat-dialog-title>{{ 'services.new' | t }}</h2>
    <form [formGroup]="form" (ngSubmit)="submit()">
      <mat-dialog-content class="d-flex flex-column gap-1">
        <mat-form-field>
          <mat-label>{{ 'common.name' | t }}</mat-label>
          <input matInput formControlName="name" maxlength="200" required cdkFocusInitial />
        </mat-form-field>
        <mat-form-field>
          <mat-label>{{ 'common.description' | t }}</mat-label>
          <textarea matInput formControlName="description" rows="3"></textarea>
        </mat-form-field>
        <div class="row g-2">
          <mat-form-field class="col-12 col-sm-6">
            <mat-label>{{ 'services.protection' | t }}</mat-label>
            <mat-select formControlName="protectionRequirementAvailability">
              <mat-option value="normal">normal</mat-option>
              <mat-option value="high">high</mat-option>
              <mat-option value="very_high">very high</mat-option>
            </mat-select>
          </mat-form-field>
          <mat-form-field class="col-12 col-sm-6">
            <mat-label>{{ 'services.impactLevel' | t }}</mat-label>
            <mat-select formControlName="impactLevel">
              <mat-option value="low">low</mat-option>
              <mat-option value="moderate">moderate</mat-option>
              <mat-option value="high">high</mat-option>
            </mat-select>
          </mat-form-field>
        </div>
        @for (e of errors(); track e) {
          <div class="status error">{{ e }}</div>
        }
      </mat-dialog-content>
      <mat-dialog-actions align="end">
        <button mat-button type="button" mat-dialog-close>{{ 'common.cancel' | t }}</button>
        <button mat-flat-button type="submit" [disabled]="form.invalid || saving()">
          {{ 'common.create' | t }}
        </button>
      </mat-dialog-actions>
    </form>
  `,
})
export class ServiceCreateDialog {
  private readonly api = inject(Api);
  private readonly ref = inject<MatDialogRef<ServiceCreateDialog, ItService>>(MatDialogRef);
  protected readonly saving = signal(false);
  protected readonly errors = signal<string[]>([]);

  protected readonly form = inject(FormBuilder).nonNullable.group({
    name: ['', [Validators.required, Validators.maxLength(200)]],
    description: [''],
    protectionRequirementAvailability: [null as ProtectionRequirement | null],
    impactLevel: [null as ImpactLevel | null],
  });

  protected submit(): void {
    if (this.form.invalid) {
      return;
    }
    this.saving.set(true);
    const v = this.form.getRawValue();
    this.api
      .createService({
        name: v.name.trim(),
        description: v.description || undefined,
        protectionRequirementAvailability: v.protectionRequirementAvailability ?? undefined,
        impactLevel: v.impactLevel ?? undefined,
      })
      .subscribe({
        next: (service) => this.ref.close(service),
        error: (e: unknown) => {
          this.errors.set(issuesOf(e));
          this.saving.set(false);
        },
      });
  }
}
