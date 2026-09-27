import { Component, computed, inject, input, linkedSignal, output } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatIconModule } from '@angular/material/icon';
import { MatTooltipModule } from '@angular/material/tooltip';
import { filter, forkJoin, map, Observable, switchMap } from 'rxjs';

import { Api } from '../../../core/api/api';
import {
  CommunicationAudience,
  CommunicationRule,
  CommunicationTrigger,
  Person,
  Role,
  RoleAssignment,
} from '../../../core/api/models';
import { injectMutation } from '../../../core/http/mutation';
import { I18n, TranslatePipe } from '../../../core/i18n/i18n';
import { ContextMenu, MenuItem } from '../../../core/ui/context-menu';
import {
  EditDialogData,
  EditResult,
  FieldValue,
  openConfirm,
  openEdit,
} from '../../../core/ui/edit-dialog';

const TRIGGERS: CommunicationTrigger[] = [
  'dr_declared',
  'status_update',
  'recovered',
  'failback',
  'aborted',
];
const AUDIENCES: CommunicationAudience[] = [
  'internal',
  'management',
  'customers',
  'regulator',
  'suppliers',
];

interface TeamData {
  roles: Role[];
  persons: Person[];
  assignments: RoleAssignment[];
  rules: CommunicationRule[];
}

/** Roles with deputies (step 10) and communication rules (step 11) of a service. */
@Component({
  selector: 'app-roles-communication',
  imports: [MatButtonModule, MatIconModule, MatTooltipModule, TranslatePipe, ContextMenu],
  template: `
    @for (e of m.issues(); track e) {
      <div class="status error mb-2">{{ e }}</div>
    }

    <!-- Roles -->
    <div class="d-flex flex-wrap align-items-center gap-2 mt-2">
      <h4 class="sub m-0 flex-grow-1">{{ 'team.roles' | t }}</h4>
      <button mat-stroked-button (click)="newPerson()" [disabled]="m.busy()">
        <mat-icon>person_add</mat-icon>{{ 'guide.owners.newPerson' | t }}
      </button>
      <button mat-stroked-button (click)="addAssignment()" [disabled]="m.busy() || !data()">
        <mat-icon>add</mat-icon>{{ 'team.addAssignment' | t }}
      </button>
    </div>
    <p class="muted small">{{ 'team.rolesHint' | t }}</p>
    <div class="table-scroll mb-3">
      <table class="grid-table">
        <thead>
          <tr>
            <th>{{ 'team.role' | t }}</th>
            <th>{{ 'team.person' | t }}</th>
            <th>{{ 'team.deputy' | t }}</th>
            <th>{{ 'team.escalation' | t }}</th>
            <th class="actions"></th>
          </tr>
        </thead>
        <tbody>
          @for (a of sortedAssignments(); track a.id) {
            <tr (contextmenu)="menu.open($event, assignmentMenu(a))" (dblclick)="editAssignment(a)">
              <td>{{ roleName(a.roleId) }}</td>
              <td>{{ personName(a.personId) }}</td>
              <td>{{ (a.isDeputy ? 'team.yes' : 'team.no') | t }}</td>
              <td>{{ a.escalationOrder ?? '–' }}</td>
              <td class="actions">
                <button
                  mat-icon-button
                  (click)="menu.open($event, assignmentMenu(a))"
                  [attr.aria-label]="'common.actions' | t"
                >
                  <mat-icon>more_vert</mat-icon>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="5" class="muted">{{ 'common.empty' | t }}</td>
            </tr>
          }
        </tbody>
      </table>
    </div>

    <!-- Communication -->
    <div class="d-flex flex-wrap align-items-center gap-2">
      <h4 class="sub m-0 flex-grow-1">{{ 'team.communication' | t }}</h4>
      <button mat-stroked-button (click)="addRule()" [disabled]="m.busy() || !data()">
        <mat-icon>add</mat-icon>{{ 'team.addRule' | t }}
      </button>
    </div>
    <p class="muted small">{{ 'team.communicationHint' | t }}</p>
    <div class="table-scroll">
      <table class="grid-table">
        <thead>
          <tr>
            <th>{{ 'team.trigger' | t }}</th>
            <th>{{ 'team.audience' | t }}</th>
            <th>{{ 'team.channel' | t }}</th>
            <th>{{ 'team.responsible' | t }}</th>
            <th>{{ 'team.authorizer' | t }}</th>
            <th>{{ 'team.frequency' | t }}</th>
            <th class="actions"></th>
          </tr>
        </thead>
        <tbody>
          @for (r of data()?.rules ?? []; track r.id) {
            <tr (contextmenu)="menu.open($event, ruleMenu(r))" (dblclick)="editRule(r)">
              <td>{{ 'team.triggers.' + r.trigger | t }}</td>
              <td>{{ 'team.audiences.' + r.audience | t }}</td>
              <td>{{ r.channel }}</td>
              <td>{{ roleName(r.responsibleRoleId) }}</td>
              <td>{{ r.authorizerRoleId ? roleName(r.authorizerRoleId) : '–' }}</td>
              <td>{{ r.frequencyMinutes ? r.frequencyMinutes + ' min' : '–' }}</td>
              <td class="actions">
                <button
                  mat-icon-button
                  (click)="menu.open($event, ruleMenu(r))"
                  [attr.aria-label]="'common.actions' | t"
                >
                  <mat-icon>more_vert</mat-icon>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="7" class="muted">{{ 'common.empty' | t }}</td>
            </tr>
          }
        </tbody>
      </table>
    </div>

    <app-context-menu #menu />
  `,
  styles: `
    .sub {
      font: var(--mat-sys-title-small);
    }
    .small {
      font: var(--mat-sys-body-small);
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
  `,
})
export class RolesCommunication {
  readonly serviceId = input.required<string>();
  readonly reloadKey = input(0);
  readonly changed = output<void>();

  private readonly api = inject(Api);
  private readonly i18n = inject(I18n);
  private readonly dialog = inject(MatDialog);
  protected readonly m = injectMutation();

  private readonly resource = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey() }),
    stream: ({ params }) =>
      forkJoin({
        roles: this.api.roles(),
        persons: this.api.persons().pipe(map((p) => p.items)),
        assignments: this.api.roleAssignments(params.id),
        rules: this.api.communicationRules(params.id),
      }),
  });
  protected readonly data = linkedSignal<TeamData | undefined, TeamData | undefined>({
    source: () => this.resource.value(),
    computation: (value, previous) => value ?? previous?.value,
  });

  /** Grouped by role, primary before deputy. */
  protected readonly sortedAssignments = computed(() =>
    [...(this.data()?.assignments ?? [])].sort(
      (a, b) =>
        this.roleName(a.roleId).localeCompare(this.roleName(b.roleId)) ||
        Number(a.isDeputy) - Number(b.isDeputy) ||
        (a.escalationOrder ?? 99) - (b.escalationOrder ?? 99),
    ),
  );

  protected roleName(id: string): string {
    return this.data()?.roles.find((r) => r.id === id)?.name ?? '–';
  }

  protected personName(id: string): string {
    return this.data()?.persons.find((p) => p.id === id)?.name ?? '–';
  }

  // ── Role assignments ──
  protected assignmentMenu(a: RoleAssignment): MenuItem[] {
    return [
      { icon: 'edit', label: 'common.edit', run: () => this.editAssignment(a) },
      { icon: 'add', label: 'team.addAssignment', run: () => this.addAssignment() },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        run: () =>
          this.remove(
            `${this.roleName(a.roleId)}: ${this.personName(a.personId)}`,
            this.api.deleteRoleAssignment(a.id),
          ),
      },
    ];
  }

  protected addAssignment(): void {
    this.dialogThen(this.assignmentDialog('team.addAssignment', { isDeputy: false }), (v) =>
      this.api.createRoleAssignment(this.serviceId(), assignment(v)),
    );
  }

  protected editAssignment(a: RoleAssignment): void {
    this.dialogThen(
      this.assignmentDialog('team.editAssignment', {
        roleId: a.roleId,
        personId: a.personId,
        isDeputy: a.isDeputy,
        escalationOrder: a.escalationOrder ?? null,
      }),
      (v) => this.api.updateRoleAssignment(a.id, assignment(v)),
    );
  }

  protected newPerson(): void {
    this.dialogThen(
      {
        title: 'guide.owners.newPerson',
        submit: 'common.create',
        fields: [
          { key: 'name', label: 'common.name', type: 'text', required: true },
          { key: 'email', label: 'guide.owners.email', type: 'text' },
          { key: 'phone', label: 'guide.owners.phone', type: 'text' },
          { key: 'alternateContact', label: 'team.alternateContact', type: 'text' },
          { key: 'team', label: 'guide.owners.team', type: 'text' },
        ],
      },
      (v) =>
        this.api.createPerson(
          Object.fromEntries(
            Object.entries(v)
              .filter(([, x]) => typeof x === 'string' && x.trim())
              .map(([k, x]) => [k, String(x).trim()]),
          ),
        ),
    );
  }

  private assignmentDialog(
    title: string,
    value: Partial<Record<string, FieldValue>>,
  ): EditDialogData {
    const d = this.data();
    return {
      title,
      fields: [
        {
          key: 'roleId',
          label: 'team.role',
          type: 'select',
          required: true,
          options: (d?.roles ?? []).map((r) => ({ value: r.id, label: r.name })),
        },
        {
          key: 'personId',
          label: 'team.person',
          type: 'select',
          required: true,
          options: (d?.persons ?? []).map((p) => ({ value: p.id, label: p.name })),
        },
        { key: 'isDeputy', label: 'team.isDeputy', type: 'checkbox' },
        {
          key: 'escalationOrder',
          label: 'team.escalation',
          type: 'number',
          min: 1,
          max: 100,
          hint: this.i18n.t('guide.optional'),
        },
      ],
      value,
    };
  }

  // ── Communication rules ──
  protected ruleMenu(r: CommunicationRule): MenuItem[] {
    return [
      { icon: 'edit', label: 'common.edit', run: () => this.editRule(r) },
      { icon: 'add', label: 'team.addRule', run: () => this.addRule() },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        run: () =>
          this.remove(
            this.i18n.t(`team.triggers.${r.trigger}`),
            this.api.deleteCommunicationRule(r.id),
          ),
      },
    ];
  }

  protected addRule(): void {
    this.dialogThen(
      this.ruleDialog('team.addRule', { trigger: 'dr_declared', audience: 'internal' }),
      (v) =>
        this.api.createCommunicationRule(this.serviceId(), {
          ...rule(v),
          authorizerRoleId: (v['authorizerRoleId'] as string | null) ?? undefined,
          frequencyMinutes: v['frequencyMinutes'] ? Number(v['frequencyMinutes']) : undefined,
        }),
    );
  }

  protected editRule(r: CommunicationRule): void {
    this.dialogThen(
      this.ruleDialog('team.editRule', {
        trigger: r.trigger,
        audience: r.audience,
        channel: r.channel,
        responsibleRoleId: r.responsibleRoleId,
        authorizerRoleId: r.authorizerRoleId ?? null,
        frequencyMinutes: r.frequencyMinutes ?? null,
        template: r.template ?? '',
      }),
      (v) =>
        this.api.updateCommunicationRule(r.id, {
          ...rule(v),
          authorizerRoleId: (v['authorizerRoleId'] as string | null) ?? null,
          frequencyMinutes: v['frequencyMinutes'] ? Number(v['frequencyMinutes']) : null,
        }),
    );
  }

  private ruleDialog(title: string, value: Partial<Record<string, FieldValue>>): EditDialogData {
    const roles = (this.data()?.roles ?? []).map((r) => ({ value: r.id, label: r.name }));
    return {
      title,
      fields: [
        {
          key: 'trigger',
          label: 'team.trigger',
          type: 'select',
          required: true,
          options: TRIGGERS.map((t) => ({ value: t, label: this.i18n.t(`team.triggers.${t}`) })),
        },
        {
          key: 'audience',
          label: 'team.audience',
          type: 'select',
          required: true,
          options: AUDIENCES.map((a) => ({ value: a, label: this.i18n.t(`team.audiences.${a}`) })),
        },
        {
          key: 'channel',
          label: 'team.channel',
          type: 'text',
          required: true,
          hint: this.i18n.t('team.channelHint'),
        },
        {
          key: 'responsibleRoleId',
          label: 'team.responsible',
          type: 'select',
          required: true,
          options: roles,
        },
        {
          key: 'authorizerRoleId',
          label: 'team.authorizer',
          type: 'select',
          hint: this.i18n.t('team.authorizerHint'),
          options: [{ value: null, label: this.i18n.t('common.none') }, ...roles],
        },
        {
          key: 'frequencyMinutes',
          label: 'team.frequency',
          type: 'number',
          min: 1,
          suffix: 'min',
          hint: this.i18n.t('team.frequencyHint'),
        },
        { key: 'template', label: 'team.template', type: 'textarea' },
      ],
      value,
    };
  }

  private dialogThen(data: EditDialogData, save: (v: EditResult) => Observable<unknown>): void {
    this.m.run(
      openEdit(this.dialog, data).pipe(
        filter((v): v is EditResult => !!v),
        switchMap(save),
      ),
      { success: 'common.saved', done: () => this.changed.emit() },
    );
  }

  private remove(name: string, request: Observable<unknown>): void {
    this.m.run(
      openConfirm(this.dialog, {
        title: 'common.deleteConfirm',
        message: name,
        confirm: 'common.delete',
      }).pipe(
        filter((ok) => !!ok),
        switchMap(() => request),
      ),
      { success: 'common.deleted', done: () => this.changed.emit() },
    );
  }
}

function assignment(v: EditResult): Partial<RoleAssignment> {
  return {
    roleId: String(v['roleId']),
    personId: String(v['personId']),
    isDeputy: !!v['isDeputy'],
    ...(v['escalationOrder'] ? { escalationOrder: Number(v['escalationOrder']) } : {}),
  };
}

function rule(v: EditResult): Partial<CommunicationRule> {
  return {
    trigger: v['trigger'] as CommunicationTrigger,
    audience: v['audience'] as CommunicationAudience,
    channel: String(v['channel']).trim(),
    responsibleRoleId: String(v['responsibleRoleId']),
    template: String(v['template'] ?? '').trim(),
  };
}
