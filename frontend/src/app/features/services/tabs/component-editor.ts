import { Injectable, inject } from '@angular/core';
import { MatDialog } from '@angular/material/dialog';
import { EMPTY, Observable, filter, switchMap } from 'rxjs';

import { Api } from '../../../core/api/api';
import { Dependency, Microservice } from '../../../core/api/models';
import { I18n } from '../../../core/i18n/i18n';
import { EditResult, FieldDef, openConfirm, openEdit } from '../../../core/ui/edit-dialog';

const DEPENDENCY_KINDS = [
  'microservice',
  'infrastructure',
  'platform',
  'external_service',
  'supplier',
  'personnel',
] as const;

/**
 * Dialogs to add, edit and delete components (API: microservices) and their dependencies. Each
 * method opens a dialog and returns the save request; it completes without a value when cancelled,
 * so callers can pass it straight to `injectMutation().run`.
 */
@Injectable({ providedIn: 'root' })
export class ComponentEditor {
  private readonly api = inject(Api);
  private readonly dialog = inject(MatDialog);
  private readonly i18n = inject(I18n);

  add(serviceId: string): Observable<Microservice> {
    return this.form('guide.components.add', {}, 'common.add').pipe(
      switchMap((v) => this.api.createMicroservice(serviceId, toComponent(v))),
    );
  }

  edit(ms: Microservice): Observable<Microservice> {
    return this.form('guide.components.edit', {
      name: ms.name,
      description: ms.description ?? '',
      platform: ms.platform ?? '',
      hostingLocation: ms.hostingLocation ?? '',
      dataStores: ms.dataStores.join(', '),
      restoreOrder: ms.restoreOrder ?? null,
    }).pipe(switchMap((v) => this.api.updateMicroservice(ms.id, toComponent(v))));
  }

  /** The default component cannot be deleted (it stands for the whole service). */
  remove(ms: Microservice): Observable<void> {
    if (ms.isDefault) {
      return EMPTY;
    }
    return openConfirm(this.dialog, {
      title: 'guide.components.deleteConfirm',
      message: ms.name,
      confirm: 'common.delete',
    }).pipe(
      filter((ok) => !!ok),
      switchMap(() => this.api.deleteMicroservice(ms.id)),
    );
  }

  /** New dependency of `ms` (it depends on the target). */
  addDependency(ms: Microservice, components: Microservice[]): Observable<Dependency> {
    const others = components.filter((c) => c.id !== ms.id);
    return openEdit(this.dialog, {
      title: 'guide.dependencies.add',
      submit: 'common.add',
      fields: [
        {
          key: 'kind',
          label: 'guide.dependencies.kind',
          type: 'select',
          required: true,
          options: DEPENDENCY_KINDS.filter((k) => k !== 'microservice' || others.length).map(
            (k) => ({ value: k, label: this.i18n.t(`dependencies.kind.${k}`) }),
          ),
        },
        {
          key: 'targetName',
          label: 'guide.dependencies.target',
          type: 'text',
          hint: this.i18n.t('guide.dependencies.targetHint'),
        },
        {
          key: 'targetMicroserviceId',
          label: 'guide.dependencies.targetComponent',
          type: 'select',
          options: others.map((c) => ({ value: c.id, label: c.name })),
        },
        {
          key: 'criticality',
          label: 'guide.dependencies.criticality',
          type: 'select',
          required: true,
          options: ['critical', 'degradable', 'optional'].map((c) => ({
            value: c,
            label: this.i18n.t(`guide.dependencies.criticalities.${c}`),
          })),
        },
        { key: 'hasOwnDrPlan', label: 'guide.dependencies.ownPlan', type: 'checkbox' },
      ],
      value: { kind: 'external_service', criticality: 'critical' },
    }).pipe(
      filter((v): v is EditResult => !!v),
      switchMap((v) => {
        const internal = v['kind'] === 'microservice';
        return this.api.createDependency(ms.id, {
          kind: v['kind'] as Dependency['kind'],
          targetMicroserviceId: internal ? String(v['targetMicroserviceId'] ?? '') : undefined,
          targetName: internal ? undefined : String(v['targetName'] ?? '').trim(),
          direction: 'upstream',
          criticality: v['criticality'] as Dependency['criticality'],
          hasOwnDrPlan: !!v['hasOwnDrPlan'],
        });
      }),
    );
  }

  private form(
    title: string,
    value: Partial<Record<string, string | number | null>>,
    submit?: string,
  ): Observable<EditResult> {
    const fields: FieldDef[] = [
      { key: 'name', label: 'common.name', type: 'text', required: true },
      { key: 'description', label: 'common.description', type: 'textarea' },
      {
        key: 'platform',
        label: 'components.platform',
        type: 'text',
        hint: this.i18n.t('guide.components.platformHint'),
      },
      { key: 'hostingLocation', label: 'components.hosting', type: 'text' },
      {
        key: 'dataStores',
        label: 'guide.components.dataStores',
        type: 'text',
        hint: this.i18n.t('guide.commaSeparated'),
      },
      { key: 'restoreOrder', label: 'components.restoreOrder', type: 'number', min: 1 },
    ];
    return openEdit(this.dialog, { title, fields, value, submit }).pipe(
      filter((v): v is EditResult => !!v),
    );
  }
}

/** Dialog value → API body. Empty strings clear optional fields. */
function toComponent(v: EditResult): Partial<Microservice> {
  return {
    name: String(v['name']).trim(),
    description: String(v['description'] ?? '').trim(),
    platform: String(v['platform'] ?? '').trim(),
    hostingLocation: String(v['hostingLocation'] ?? '').trim(),
    dataStores: String(v['dataStores'] ?? '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean),
    restoreOrder:
      v['restoreOrder'] === null || v['restoreOrder'] === ''
        ? undefined
        : Number(v['restoreOrder']),
  };
}
