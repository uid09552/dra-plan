import { DatePipe } from '@angular/common';
import { Component, inject, signal } from '@angular/core';
import { rxResource, toObservable, toSignal } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatTableModule } from '@angular/material/table';
import { Router, RouterLink } from '@angular/router';
import { debounceTime, map } from 'rxjs';

import { Api } from '../../core/api/api';
import { I18n, TranslatePipe } from '../../core/i18n/i18n';
import { ServiceCreateDialog } from './service-create-dialog';
import { PlanStatusChip } from './status-chips';

@Component({
  selector: 'app-services-list',
  imports: [
    RouterLink,
    DatePipe,
    MatTableModule,
    MatButtonModule,
    MatIconModule,
    MatFormFieldModule,
    MatInputModule,
    MatProgressBarModule,
    TranslatePipe,
    PlanStatusChip,
  ],
  templateUrl: './services-list.html',
  styles: `
    .toolbar {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      align-items: center;
      margin-bottom: 8px;
    }
    .search {
      flex: 1 1 260px;
      max-width: 420px;
    }
    tr.mat-mdc-row {
      cursor: pointer;
    }
    tr.mat-mdc-row:hover {
      background: var(--mat-sys-surface-container-high);
    }
    .progress {
      min-width: 120px;
    }
  `,
})
export class ServicesList {
  private readonly api = inject(Api);
  private readonly dialog = inject(MatDialog);
  private readonly router = inject(Router);
  private readonly snack = inject(MatSnackBar);
  private readonly i18n = inject(I18n);

  protected readonly query = signal('');
  private readonly debouncedQuery = toSignal(toObservable(this.query).pipe(debounceTime(250)), {
    initialValue: '',
  });
  protected readonly services = rxResource({
    params: () => this.debouncedQuery(),
    stream: ({ params }) => this.api.services(params.trim() || undefined).pipe(map((p) => p.items)),
  });
  protected readonly columns = ['name', 'protection', 'workflow', 'plan', 'review'];

  protected create(): void {
    this.dialog
      .open(ServiceCreateDialog, { width: '560px', maxWidth: '95vw' })
      .afterClosed()
      .subscribe((service) => {
        if (service) {
          this.snack.open(this.i18n.t('services.created'), undefined, { duration: 3000 });
          void this.router.navigate(['/services', service.id]);
        }
      });
  }

  protected open(id: string): void {
    void this.router.navigate(['/services', id]);
  }
}
