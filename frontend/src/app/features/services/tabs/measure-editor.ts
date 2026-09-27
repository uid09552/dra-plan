import { Injectable, inject } from '@angular/core';
import { MatDialog } from '@angular/material/dialog';
import { Observable, filter, switchMap } from 'rxjs';

import { Api } from '../../../core/api/api';
import {
  Catalog,
  ImplementationStatus,
  Microservice,
  RecoveryStrategy,
  Scenario,
} from '../../../core/api/models';
import { I18n } from '../../../core/i18n/i18n';
import { EditResult, FieldDef, openConfirm, openEdit } from '../../../core/ui/edit-dialog';

export const IMPLEMENTATION_STATUSES: ImplementationStatus[] = [
  'not_implemented',
  'in_progress',
  'implemented',
];

/** What the measure dialogs need to offer choices. */
export interface MeasureContext {
  components: Microservice[];
  scenarios: Scenario[];
  catalog?: Catalog;
}

/**
 * Dialogs for measures (recovery strategies): add, edit, select (with gap acceptance) and delete.
 * Each method returns the save request and completes without a value when cancelled, so callers
 * can pass it to `injectMutation().run`.
 */
@Injectable({ providedIn: 'root' })
export class MeasureEditor {
  private readonly api = inject(Api);
  private readonly dialog = inject(MatDialog);
  private readonly i18n = inject(I18n);

  add(
    ctx: MeasureContext,
    preset: { scenarioIds?: string[]; microserviceId?: string } = {},
  ): Observable<RecoveryStrategy> {
    const fields: FieldDef[] = [
      {
        key: 'microserviceId',
        label: 'guide.components.component',
        type: 'select',
        required: true,
        options: ctx.components.map((c) => ({ value: c.id, label: c.name })),
      },
      ...this.fields(ctx),
    ];
    const defaultComponent = ctx.components.find((c) => c.isDefault)?.id ?? null;
    return openEdit(this.dialog, {
      title: 'measures.add',
      submit: 'common.add',
      fields,
      value: {
        microserviceId: preset.microserviceId ?? defaultComponent,
        scenarioIds: preset.scenarioIds ?? [],
        implementationStatus: 'not_implemented',
      },
    }).pipe(
      filter((v): v is EditResult => !!v),
      switchMap((v) =>
        this.api.createStrategy(String(v['microserviceId']), {
          ...toMeasure(v),
          scenarioIds: v['scenarioIds'] as string[],
          type: String(v['type']),
        }),
      ),
    );
  }

  edit(st: RecoveryStrategy, ctx: MeasureContext): Observable<RecoveryStrategy> {
    return openEdit(this.dialog, {
      title: 'measures.edit',
      fields: this.fields(ctx),
      value: {
        scenarioIds: st.scenarioIds,
        type: st.type,
        title: st.title ?? '',
        estimatedRtoMinutes: st.estimatedRtoMinutes,
        estimatedRpoMinutes: st.estimatedRpoMinutes,
        implementationStatus: st.implementationStatus,
        lastTestedAt: st.lastTestedAt?.slice(0, 10) ?? '',
      },
    }).pipe(
      filter((v): v is EditResult => !!v),
      switchMap((v) =>
        this.api.updateStrategy(st.id, {
          ...toMeasure(v),
          scenarioIds: v['scenarioIds'] as string[],
          type: String(v['type']),
          lastTestedAt: v['lastTestedAt']
            ? new Date(String(v['lastTestedAt'])).toISOString()
            : null,
        }),
      ),
    );
  }

  /** Selects the measure for the plan; a gap must be accepted with a justification. */
  select(st: RecoveryStrategy): Observable<RecoveryStrategy> {
    if (st.gapCheck?.status !== 'gap') {
      return this.api.selectStrategy(st.id);
    }
    return openEdit(this.dialog, {
      title: 'measures.acceptGap',
      submit: 'measures.acceptGapSubmit',
      fields: [
        {
          key: 'gapJustification',
          label: 'measures.gapJustification',
          type: 'textarea',
          required: true,
          hint: this.i18n.t('measures.gapHint', {
            rto: Math.max(0, st.gapCheck.rtoGapMinutes ?? 0),
          }),
        },
      ],
    }).pipe(
      filter((v): v is EditResult => !!v),
      switchMap((v) =>
        this.api.selectStrategy(st.id, {
          acceptGap: true,
          gapJustification: String(v['gapJustification']).trim(),
        }),
      ),
    );
  }

  remove(st: RecoveryStrategy, label: string): Observable<void> {
    return openConfirm(this.dialog, {
      title: 'measures.deleteConfirm',
      message: label,
      confirm: 'common.delete',
    }).pipe(
      filter((ok) => !!ok),
      switchMap(() => this.api.deleteStrategy(st.id)),
    );
  }

  /** Label of a measure: its title, else the strategy type. */
  label(st: RecoveryStrategy, catalog?: Catalog): string {
    return st.title ?? catalog?.strategyTypes.find((t) => t.key === st.type)?.label ?? st.type;
  }

  private fields(ctx: MeasureContext): FieldDef[] {
    const scenarios = ctx.scenarios.filter((s) => s.status !== 'merged');
    return [
      {
        key: 'scenarioIds',
        label: 'measures.scenarios',
        type: 'multiselect',
        required: true,
        hint: this.i18n.t('measures.scenariosHint'),
        options: scenarios.map((s) => ({
          value: s.id,
          label:
            s.status === 'selected'
              ? s.title
              : `${s.title} (${this.i18n.t(`scenarioMap.status.${s.status}`)})`,
        })),
      },
      {
        key: 'type',
        label: 'guide.mitigations.type',
        type: 'select',
        required: true,
        options: (ctx.catalog?.strategyTypes ?? []).map((t) => ({
          value: t.key,
          label: `${t.label} (${t.typicalRto})`,
        })),
      },
      { key: 'title', label: 'common.name', type: 'text' },
      {
        key: 'estimatedRtoMinutes',
        label: 'guide.mitigations.estRto',
        type: 'number',
        required: true,
        min: 0,
        suffix: 'min',
      },
      {
        key: 'estimatedRpoMinutes',
        label: 'guide.mitigations.estRpo',
        type: 'number',
        required: true,
        min: 0,
        suffix: 'min',
      },
      {
        key: 'implementationStatus',
        label: 'guide.mitigations.implementation',
        type: 'select',
        required: true,
        options: IMPLEMENTATION_STATUSES.map((s) => ({
          value: s,
          label: this.i18n.t(`guide.mitigations.status.${s}`),
        })),
      },
      { key: 'lastTestedAt', label: 'guide.mitigations.lastTested', type: 'date' },
    ];
  }
}

function toMeasure(v: EditResult): Partial<RecoveryStrategy> {
  return {
    title: String(v['title'] ?? '').trim(),
    estimatedRtoMinutes: Number(v['estimatedRtoMinutes']),
    estimatedRpoMinutes: Number(v['estimatedRpoMinutes']),
    implementationStatus: v['implementationStatus'] as ImplementationStatus,
    ...(v['lastTestedAt']
      ? { lastTestedAt: new Date(String(v['lastTestedAt'])).toISOString() }
      : {}),
  };
}
