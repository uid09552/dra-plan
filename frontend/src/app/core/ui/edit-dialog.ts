import { Component, inject } from '@angular/core';
import {
  FormBuilder,
  FormGroup,
  ReactiveFormsModule,
  ValidatorFn,
  Validators,
} from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatCheckboxModule } from '@angular/material/checkbox';
import {
  MAT_DIALOG_DATA,
  MatDialog,
  MatDialogModule,
  MatDialogRef,
} from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';
import { MatSelectModule } from '@angular/material/select';
import { Observable } from 'rxjs';

import { TranslatePipe } from '../i18n/i18n';

export type FieldValue = string | number | boolean | null | string[];

export interface FieldDef {
  key: string;
  /** i18n key (or plain text). */
  label: string;
  type: 'text' | 'textarea' | 'number' | 'select' | 'multiselect' | 'date' | 'checkbox';
  required?: boolean;
  /** Option labels are shown as given (translate them before). */
  options?: { value: string | number | null; label: string }[];
  hint?: string;
  suffix?: string;
  min?: number;
  max?: number;
}

export interface EditDialogData {
  /** i18n key. */
  title: string;
  fields: FieldDef[];
  value?: Partial<Record<string, FieldValue>>;
  /** i18n key of the submit button (default: save). */
  submit?: string;
}

export type EditResult = Record<string, FieldValue>;

/** Generic form dialog for create/edit from context menus. Closes with the form value. */
@Component({
  selector: 'app-edit-dialog',
  imports: [
    ReactiveFormsModule,
    MatDialogModule,
    MatButtonModule,
    MatCheckboxModule,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
    TranslatePipe,
  ],
  template: `
    <h2 mat-dialog-title>{{ data.title | t }}</h2>
    <form [formGroup]="form" (ngSubmit)="submit()">
      <mat-dialog-content class="fields">
        @for (f of data.fields; track f.key) {
          @switch (f.type) {
            @case ('checkbox') {
              <mat-checkbox [formControlName]="f.key">{{ f.label | t }}</mat-checkbox>
            }
            @case ('select') {
              <mat-form-field>
                <mat-label>{{ f.label | t }}</mat-label>
                <mat-select [formControlName]="f.key" [required]="!!f.required">
                  @for (o of f.options ?? []; track o.value) {
                    <mat-option [value]="o.value">{{ o.label }}</mat-option>
                  }
                </mat-select>
                <mat-hint>{{ hint(f) }}</mat-hint>
              </mat-form-field>
            }
            @case ('multiselect') {
              <mat-form-field>
                <mat-label>{{ f.label | t }}</mat-label>
                <mat-select [formControlName]="f.key" multiple>
                  @for (o of f.options ?? []; track o.value) {
                    <mat-option [value]="o.value">{{ o.label }}</mat-option>
                  }
                </mat-select>
                <mat-hint>{{ hint(f) }}</mat-hint>
              </mat-form-field>
            }
            @case ('textarea') {
              <mat-form-field>
                <mat-label>{{ f.label | t }}</mat-label>
                <textarea
                  matInput
                  rows="3"
                  [formControlName]="f.key"
                  [required]="!!f.required"
                ></textarea>
                <mat-hint>{{ hint(f) }}</mat-hint>
              </mat-form-field>
            }
            @default {
              <mat-form-field>
                <mat-label>{{ f.label | t }}</mat-label>
                <input
                  matInput
                  [type]="f.type"
                  [formControlName]="f.key"
                  [required]="!!f.required"
                  [attr.min]="f.min"
                  [attr.max]="f.max"
                />
                @if (f.suffix) {
                  <span matTextSuffix>{{ f.suffix }}</span>
                }
                <mat-hint>{{ hint(f) }}</mat-hint>
              </mat-form-field>
            }
          }
        }
      </mat-dialog-content>
      <mat-dialog-actions align="end">
        <button mat-button type="button" mat-dialog-close>{{ 'common.cancel' | t }}</button>
        <button mat-flat-button type="submit" [disabled]="form.invalid">
          {{ data.submit ?? 'common.save' | t }}
        </button>
      </mat-dialog-actions>
    </form>
  `,
  styles: `
    .fields {
      display: flex;
      flex-direction: column;
      gap: 4px;
      min-width: min(420px, 80vw);
      padding-top: 8px;
    }
  `,
})
export class EditDialog {
  protected readonly data = inject<EditDialogData>(MAT_DIALOG_DATA);
  private readonly ref = inject<MatDialogRef<EditDialog, EditResult>>(MatDialogRef);
  protected readonly form: FormGroup = inject(FormBuilder).group(
    Object.fromEntries(
      this.data.fields.map((f) => [
        f.key,
        [this.data.value?.[f.key] ?? defaultOf(f), validators(f)],
      ]),
    ),
  );

  protected hint(f: FieldDef): string {
    return f.hint ?? '';
  }

  protected submit(): void {
    if (this.form.valid) {
      this.ref.close(this.form.getRawValue() as EditResult);
    }
  }
}

function defaultOf(f: FieldDef): FieldValue {
  if (f.type === 'checkbox') {
    return false;
  }
  if (f.type === 'multiselect') {
    return [];
  }
  return f.type === 'select' || f.type === 'number' ? null : '';
}

function validators(f: FieldDef): ValidatorFn[] {
  const v: ValidatorFn[] = [];
  if (f.required) {
    v.push(f.type === 'checkbox' ? Validators.requiredTrue : Validators.required);
  }
  if (f.min !== undefined) {
    v.push(Validators.min(f.min));
  }
  if (f.max !== undefined) {
    v.push(Validators.max(f.max));
  }
  return v;
}

export interface ConfirmData {
  /** i18n key. */
  title: string;
  /** Plain text (e.g. the item name). */
  message: string;
  /** i18n key of the confirm button. */
  confirm: string;
}

@Component({
  selector: 'app-confirm-dialog',
  imports: [MatDialogModule, MatButtonModule, TranslatePipe],
  template: `
    <h2 mat-dialog-title>{{ data.title | t }}</h2>
    <mat-dialog-content>{{ data.message }}</mat-dialog-content>
    <mat-dialog-actions align="end">
      <button mat-button [mat-dialog-close]="false">{{ 'common.cancel' | t }}</button>
      <button mat-flat-button class="danger" [mat-dialog-close]="true" cdkFocusInitial>
        {{ data.confirm | t }}
      </button>
    </mat-dialog-actions>
  `,
  styles: `
    .danger {
      --mat-button-filled-container-color: var(--mat-sys-error);
      --mat-button-filled-label-text-color: var(--mat-sys-on-error);
    }
  `,
})
export class ConfirmDialog {
  protected readonly data = inject<ConfirmData>(MAT_DIALOG_DATA);
}

/** Opens the edit dialog; emits the form value, or `undefined` when cancelled. */
export function openEdit(
  dialog: MatDialog,
  data: EditDialogData,
): Observable<EditResult | undefined> {
  return dialog
    .open<EditDialog, EditDialogData, EditResult>(EditDialog, { data, autoFocus: 'first-tabbable' })
    .afterClosed();
}

export function openConfirm(dialog: MatDialog, data: ConfirmData): Observable<boolean | undefined> {
  return dialog.open<ConfirmDialog, ConfirmData, boolean>(ConfirmDialog, { data }).afterClosed();
}
