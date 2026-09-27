import { DatePipe } from '@angular/common';
import {
  Component,
  computed,
  effect,
  inject,
  input,
  linkedSignal,
  output,
  signal,
} from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { FormBuilder, ReactiveFormsModule, Validators } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatCheckboxModule } from '@angular/material/checkbox';
import { MatDialog } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSelectModule } from '@angular/material/select';
import { MatTooltipModule } from '@angular/material/tooltip';
import { concatMap, forkJoin, from, map, Observable, of, switchMap, toArray } from 'rxjs';

import { Api } from '../../../core/api/api';
import {
  Bia,
  Dependency,
  Issue,
  ItService,
  Microservice,
  Person,
  RecoveryObjective,
  Scenario,
  ServiceReadiness,
  Tenant,
  WorkflowState,
  WorkflowStep,
} from '../../../core/api/models';
import { injectMutation } from '../../../core/http/mutation';
import { I18n, TranslatePipe } from '../../../core/i18n/i18n';
import { ContextMenu, MenuItem } from '../../../core/ui/context-menu';
import { EditDialogData, EditResult, openConfirm, openEdit } from '../../../core/ui/edit-dialog';
import { StepStatusChip } from '../status-chips';
import { ComponentEditor } from './component-editor';
import { MeasuresView } from './measures-view';
import { RolesCommunication } from './roles-communication';

/** Wizard stages and the workflow steps (gates) each one covers. */
export const STAGES = [
  { key: 'service', icon: 'badge', steps: ['define_service'] },
  { key: 'bia', icon: 'monitoring', steps: ['business_impact'] },
  { key: 'dependencies', icon: 'hub', steps: ['map_dependencies', 'recovery_objectives'] },
  {
    key: 'scenarios',
    icon: 'account_tree',
    steps: ['brainstorm_scenarios', 'consolidate_scenarios', 'select_scenarios'],
  },
  { key: 'mitigations', icon: 'shield', steps: ['recovery_strategies', 'runbooks'] },
  { key: 'plan', icon: 'description', steps: ['roles', 'communication', 'review_approve'] },
  { key: 'validate', icon: 'science', steps: ['test', 'measure', 'improve'] },
] as const;

export type StageKey = (typeof STAGES)[number]['key'];
export type StageState = 'done' | 'blocked' | 'open';

/** Tabs of the service page the guide can link to. */
export type ServiceTab =
  'scenarios' | 'measures' | 'dependencies' | 'handbook' | 'compliance' | 'plans';

interface GuideData {
  service: ItService;
  tenant: Tenant;
  persons: Person[];
  bia: Bia | null;
  microservices: Microservice[];
  dependencies: Dependency[];
  objectives: RecoveryObjective[];
  scenarios: Scenario[];
  workflow: WorkflowState;
  readiness: ServiceReadiness;
}

/** State of a stage from its workflow steps: done when all are complete. */
export function stageState(steps: readonly string[], workflow: WorkflowState): StageState {
  const mapped = workflow.steps.filter((s) => steps.includes(s.key));
  if (mapped.length && mapped.every((s) => s.status === 'complete')) {
    return 'done';
  }
  return mapped.some((s) => s.issues.some((i) => i.severity === 'blocking')) ? 'blocked' : 'open';
}

const HOURS = 60;

/** Color of a readiness score (0–100). */
export function scoreTone(score: number): 'ok' | 'warn' | 'error' {
  if (score >= 80) {
    return 'ok';
  }
  return score >= 40 ? 'warn' : 'error';
}

const STATE_ICON: Partial<Record<StageState, string>> = { done: 'check_circle', blocked: 'error' };

@Component({
  selector: 'app-service-guide',
  imports: [
    DatePipe,
    ReactiveFormsModule,
    MatButtonModule,
    MatCardModule,
    MatCheckboxModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
    MatSelectModule,
    MatTooltipModule,
    TranslatePipe,
    ContextMenu,
    StepStatusChip,
    MeasuresView,
    RolesCommunication,
  ],
  templateUrl: './service-guide.html',
  styleUrl: './service-guide.scss',
})
export class ServiceGuide {
  readonly serviceId = input.required<string>();
  readonly reloadKey = input(0);
  readonly changed = output<void>();
  readonly openTab = output<ServiceTab>();

  private readonly api = inject(Api);
  private readonly i18n = inject(I18n);
  private readonly fb = inject(FormBuilder).nonNullable;
  private readonly dialog = inject(MatDialog);
  private readonly components = inject(ComponentEditor);
  protected readonly m = injectMutation();

  private readonly resource = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey() }),
    stream: ({ params }) => this.load(params.id),
  });
  /** Keeps the previous data while reloading, so the wizard does not flicker. */
  protected readonly data = linkedSignal<GuideData | undefined, GuideData | undefined>({
    source: () => this.resource.value(),
    computation: (value, previous) => value ?? previous?.value,
  });
  protected readonly loading = computed(() => this.resource.isLoading());
  protected readonly suggestions = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey(), lang: this.i18n.lang() }),
    stream: ({ params }) => this.api.scenarioSuggestions(params.id),
  });

  protected readonly stages = computed(() => {
    const workflow = this.data()?.workflow;
    return STAGES.map((stage, index) => ({
      ...stage,
      index,
      state: workflow ? stageState(stage.steps, workflow) : ('open' as StageState),
    }));
  });
  /** Explicitly chosen stage; defaults to the first one that is not done. */
  private readonly chosen = signal<StageKey | null>(null);
  protected readonly current = computed(() => {
    const stages = this.stages();
    const key = this.chosen() ?? stages.find((s) => s.state !== 'done')?.key ?? 'plan';
    return stages.find((s) => s.key === key) ?? stages[0];
  });
  protected readonly next = computed(() => this.stages()[this.current().index + 1]);

  /** Issues of the current stage's workflow steps, blocking first. */
  protected readonly issues = computed<Issue[]>(() => {
    const steps: readonly string[] = this.current().steps;
    return (this.data()?.workflow.steps ?? [])
      .filter((s) => steps.includes(s.key))
      .flatMap((s) => s.issues)
      .sort((a, b) => (a.severity === 'blocking' ? 0 : 1) - (b.severity === 'blocking' ? 0 : 1));
  });
  /** Workflow steps (gates) of the current stage. */
  protected readonly stageSteps = computed(() => {
    const steps: readonly string[] = this.current().steps;
    return (this.data()?.workflow.steps ?? []).filter((s) => steps.includes(s.key));
  });
  protected readonly incompleteSteps = computed(() => {
    const steps: readonly string[] = this.current().steps;
    return (this.data()?.workflow.steps ?? []).filter(
      (s) => steps.includes(s.key) && s.status !== 'complete',
    );
  });

  // ── Stage 1: service ──
  protected readonly serviceForm = this.fb.group({
    name: ['', Validators.required],
    description: [''],
    businessOwnerId: [''],
    technicalOwnerId: [''],
    protectionRequirementAvailability: ['', Validators.required],
    impactLevel: [''],
  });

  // ── Stage 2: BIA & assets ──
  protected readonly biaForm = this.fb.group({
    mtpdHours: [null as number | null, [Validators.required, Validators.min(0)]],
    rtoHours: [null as number | null, [Validators.required, Validators.min(0)]],
    rpoMinutes: [null as number | null, [Validators.required, Validators.min(0)]],
    minimumOperatingLevel: [''],
    regulatoryRequirements: [''],
  });
  /** Impact level per `category|timeWindow`. */
  protected readonly ratings = signal<Partial<Record<string, number>>>({});
  // ── Stage 3: dependencies & objectives ──
  protected readonly depForm = this.fb.group({
    microserviceId: ['', Validators.required],
    kind: ['external_service', Validators.required],
    target: ['', Validators.required],
    criticality: ['critical', Validators.required],
    hasOwnDrPlan: [false],
  });
  protected readonly dependencyKinds = [
    'microservice',
    'infrastructure',
    'platform',
    'external_service',
    'supplier',
    'personnel',
  ];
  protected readonly objectiveDrafts = signal<
    Partial<Record<string, { rto?: number; rpo?: number }>>
  >({});

  protected readonly scenarioCounts = computed(() => {
    const list = (this.data()?.scenarios ?? []).filter((s) => s.status !== 'merged');
    return {
      total: list.length,
      open: list.filter((s) => s.status === 'brainstormed').length,
      selected: list.filter((s) => s.status === 'selected').length,
      unrated: list.filter((s) => !s.likelihood || !s.impact).length,
    };
  });
  constructor() {
    effect(() => {
      const d = this.data();
      if (!d) {
        return;
      }
      this.serviceForm.reset({
        name: d.service.name,
        description: d.service.description ?? '',
        businessOwnerId: d.service.businessOwnerId ?? '',
        technicalOwnerId: d.service.technicalOwnerId ?? '',
        protectionRequirementAvailability: d.service.protectionRequirementAvailability ?? '',
        impactLevel: d.service.impactLevel ?? '',
      });
      if (d.bia) {
        this.biaForm.reset({
          mtpdHours: d.bia.mtpdMinutes / HOURS,
          rtoHours: d.bia.serviceRtoMinutes / HOURS,
          rpoMinutes: d.bia.serviceRpoMinutes,
          minimumOperatingLevel: d.bia.minimumOperatingLevel ?? '',
          regulatoryRequirements: d.bia.regulatoryRequirements ?? '',
        });
        this.ratings.set(
          Object.fromEntries(
            d.bia.impactRatings.map((r) => [`${r.impactCategory}|${r.timeWindowMinutes}`, r.level]),
          ),
        );
      }
      if (!this.depForm.value.microserviceId && d.microservices.length) {
        this.depForm.patchValue({ microserviceId: d.microservices[0].id });
      }
    });
  }

  protected readonly scoreTone = scoreTone;

  protected crumbIcon(state: StageState, icon: string): string {
    return STATE_ICON[state] ?? icon;
  }

  protected go(key: StageKey): void {
    this.chosen.set(key);
    this.m.clear();
  }

  protected stepTitle(key: string): string {
    return this.data()?.workflow.steps.find((s) => s.key === key)?.title ?? key;
  }

  protected completeStep(step: WorkflowStep): void {
    this.m.run(this.api.completeStep(this.serviceId(), step.key, true), {
      success: 'workflow.completed',
      done: () => this.changed.emit(),
    });
  }

  protected reopenStep(step: WorkflowStep): void {
    this.m.run(this.api.reopenStep(this.serviceId(), step.key), {
      done: () => this.changed.emit(),
    });
  }

  /** Completes the stage's open workflow steps in order (warnings acknowledged). */
  protected completeStage(): void {
    const keys = this.incompleteSteps().map((s) => s.key);
    this.m.run(
      from(keys).pipe(
        concatMap((k) => this.api.completeStep(this.serviceId(), k, true)),
        toArray(),
      ),
      { success: 'guide.stageCompleted', done: () => this.changed.emit() },
    );
  }

  protected continue(): void {
    const next = this.next();
    if (next) {
      this.go(next.key);
    }
  }

  // ── Stage 1 ──
  protected saveService(): void {
    const v = this.serviceForm.getRawValue();
    this.m.run(
      this.api.updateService(this.serviceId(), {
        name: v.name.trim(),
        description: v.description.trim(),
        businessOwnerId: v.businessOwnerId || null,
        technicalOwnerId: v.technicalOwnerId || null,
        protectionRequirementAvailability:
          v.protectionRequirementAvailability as ItService['protectionRequirementAvailability'],
        impactLevel: (v.impactLevel || undefined) as ItService['impactLevel'],
      }),
      { success: 'common.saved', done: () => this.changed.emit() },
    );
  }

  /** Creates a contact and sets it as business or technical owner. */
  protected newOwner(field: 'businessOwnerId' | 'technicalOwnerId'): void {
    this.edit(
      {
        title: 'guide.owners.newPerson',
        submit: 'common.create',
        fields: [
          { key: 'name', label: 'common.name', type: 'text', required: true },
          { key: 'email', label: 'guide.owners.email', type: 'text' },
          { key: 'phone', label: 'guide.owners.phone', type: 'text' },
          { key: 'team', label: 'guide.owners.team', type: 'text' },
        ],
      },
      (v) =>
        this.api
          .createPerson(clean(v))
          .pipe(switchMap((p) => this.api.updateService(this.serviceId(), { [field]: p.id }))),
    );
  }

  // ── Row menus (right click or ⋮) ──
  protected msMenu(ms: Microservice): MenuItem[] {
    const all = this.data()?.microservices ?? [];
    return [
      { icon: 'edit', label: 'common.edit', run: () => this.editComponent(ms) },
      {
        icon: 'hub',
        label: 'guide.dependencies.add',
        run: () => this.save(this.components.addDependency(ms, all)),
      },
      { icon: 'add', label: 'guide.components.add', run: () => this.addComponent() },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        // The default component stands for the whole service.
        disabled: ms.isDefault,
        run: () => this.save(this.components.remove(ms), 'common.deleted'),
      },
    ];
  }

  protected addComponent(): void {
    this.save(this.components.add(this.serviceId()));
  }

  protected editComponent(ms: Microservice): void {
    this.save(this.components.edit(ms));
  }

  private save(request: Observable<unknown>, success = 'common.saved'): void {
    this.m.run(request, { success, done: () => this.changed.emit() });
  }

  protected dependencyMenu(dep: Dependency): MenuItem[] {
    return [
      { icon: 'edit', label: 'common.edit', run: () => this.editDependency(dep) },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        run: () =>
          this.remove(
            dep.targetName ?? this.msName(dep.targetMicroserviceId),
            this.api.deleteDependency(dep.id),
          ),
      },
    ];
  }

  protected objectiveMenu(ms: Microservice, o: RecoveryObjective): MenuItem[] {
    return [
      {
        icon: 'edit',
        label: 'common.edit',
        run: () =>
          this.edit(
            {
              title: 'guide.objectives.title',
              fields: [
                {
                  key: 'rtoMinutes',
                  label: 'RTO',
                  type: 'number',
                  required: true,
                  min: 0,
                  suffix: 'min',
                },
                {
                  key: 'rpoMinutes',
                  label: 'RPO',
                  type: 'number',
                  required: true,
                  min: 0,
                  suffix: 'min',
                },
              ],
              value: { rtoMinutes: o.rtoMinutes, rpoMinutes: o.rpoMinutes },
            },
            (v) =>
              this.api.updateObjective(o.id, {
                rtoMinutes: Number(v['rtoMinutes']),
                rpoMinutes: Number(v['rpoMinutes']),
              }),
          ),
      },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        run: () => this.remove(`${ms.name}: RTO/RPO`, this.api.deleteObjective(o.id)),
      },
    ];
  }

  private editDependency(dep: Dependency): void {
    const criticality = ['critical', 'degradable', 'optional'].map((c) => ({
      value: c,
      label: this.i18n.t(`guide.dependencies.criticalities.${c}`),
    }));
    this.edit(
      {
        title: 'guide.dependencies.edit',
        fields: [
          ...(dep.kind === 'microservice'
            ? []
            : [
                {
                  key: 'targetName',
                  label: 'guide.dependencies.target',
                  type: 'text' as const,
                  required: true,
                },
              ]),
          {
            key: 'criticality',
            label: 'guide.dependencies.criticality',
            type: 'select',
            required: true,
            options: criticality,
          },
          { key: 'hasOwnDrPlan', label: 'guide.dependencies.ownPlan', type: 'checkbox' },
        ],
        value: {
          targetName: dep.targetName ?? '',
          criticality: dep.criticality,
          hasOwnDrPlan: dep.hasOwnDrPlan,
        },
      },
      (v) =>
        this.api.updateDependency(dep.id, {
          ...(dep.kind === 'microservice' ? {} : { targetName: String(v['targetName']).trim() }),
          criticality: v['criticality'] as Dependency['criticality'],
          hasOwnDrPlan: !!v['hasOwnDrPlan'],
        }),
    );
  }

  /** Opens the edit dialog and saves on submit. */
  private edit(data: EditDialogData, save: (value: EditResult) => Observable<unknown>): void {
    openEdit(this.dialog, data).subscribe((value) => {
      if (value) {
        this.m.run(save(value), { success: 'common.saved', done: () => this.changed.emit() });
      }
    });
  }

  private remove(name: string, request: Observable<unknown>): void {
    openConfirm(this.dialog, {
      title: 'common.deleteConfirm',
      message: name,
      confirm: 'common.delete',
    }).subscribe((ok) => {
      if (ok) {
        this.m.run(request, { success: 'common.deleted', done: () => this.changed.emit() });
      }
    });
  }

  // ── Stage 2 ──
  protected ratingKey(category: string, window: number): string {
    return `${category}|${window}`;
  }

  protected setRating(category: string, window: number, level: number | null): void {
    this.ratings.update((r) => ({ ...r, [this.ratingKey(category, window)]: level ?? undefined }));
  }

  protected saveBia(): void {
    const v = this.biaForm.getRawValue();
    const impactRatings = Object.entries(this.ratings())
      .filter((e): e is [string, number] => typeof e[1] === 'number')
      .map(([key, level]) => {
        const [impactCategory, window] = key.split('|');
        return { impactCategory, timeWindowMinutes: Number(window), level };
      });
    this.m.run(
      this.api.putBia(this.serviceId(), {
        mtpdMinutes: Math.round((v.mtpdHours ?? 0) * HOURS),
        serviceRtoMinutes: Math.round((v.rtoHours ?? 0) * HOURS),
        serviceRpoMinutes: Math.round(v.rpoMinutes ?? 0),
        minimumOperatingLevel: v.minimumOperatingLevel.trim() || undefined,
        regulatoryRequirements: v.regulatoryRequirements.trim() || undefined,
        impactRatings,
      }),
      { success: 'common.saved', done: () => this.changed.emit() },
    );
  }

  protected windowLabel(minutes: number): string {
    if (minutes % 1440 === 0) {
      return `${minutes / 1440} d`;
    }
    return minutes % 60 === 0 ? `${minutes / 60} h` : `${minutes} min`;
  }

  // ── Stage 3 ──
  protected msName(id?: string): string {
    return this.data()?.microservices.find((m) => m.id === id)?.name ?? '–';
  }

  protected addDependency(): void {
    const v = this.depForm.getRawValue();
    const internal = v.kind === 'microservice';
    this.m.run(
      this.api.createDependency(v.microserviceId, {
        kind: v.kind as Dependency['kind'],
        targetMicroserviceId: internal ? v.target : undefined,
        targetName: internal ? undefined : v.target.trim(),
        direction: 'upstream',
        criticality: v.criticality as Dependency['criticality'],
        hasOwnDrPlan: v.hasOwnDrPlan,
      }),
      {
        success: 'common.saved',
        done: () => {
          this.depForm.patchValue({ target: '', hasOwnDrPlan: false });
          this.changed.emit();
        },
      },
    );
  }

  protected defaultObjective(msId: string): RecoveryObjective | undefined {
    return this.data()?.objectives.find((o) => o.microserviceId === msId && !o.scenarioId);
  }

  protected setDraft(msId: string, field: 'rto' | 'rpo', value: string): void {
    const n = value === '' ? undefined : Number(value);
    this.objectiveDrafts.update((d) => ({ ...d, [msId]: { ...d[msId], [field]: n } }));
  }

  protected saveObjective(msId: string): void {
    const draft = this.objectiveDrafts()[msId];
    if (draft?.rto === undefined || draft.rpo === undefined) {
      return;
    }
    this.m.run(this.api.createObjective(msId, { rtoMinutes: draft.rto, rpoMinutes: draft.rpo }), {
      success: 'common.saved',
      done: () => this.changed.emit(),
    });
  }

  // ── Stage 4 ──
  protected acceptSuggestion(templateId: string): void {
    this.m.run(this.api.createScenario(this.serviceId(), { catalogTemplateId: templateId }), {
      success: 'scenarioMap.added',
      done: () => this.changed.emit(),
    });
  }

  // ── Stage 6 ──
  protected submitPlan(): void {
    this.m.run(this.api.submitPlan(this.serviceId()), {
      success: 'plans.submitted',
      done: () => this.changed.emit(),
    });
  }

  private load(id: string): Observable<GuideData> {
    return this.api.microservices(id).pipe(
      switchMap((microservices) => {
        const perMs = <T>(f: (msId: string) => Observable<T[]>): Observable<T[]> =>
          microservices.length
            ? forkJoin(microservices.map((m) => f(m.id))).pipe(map((all) => all.flat()))
            : of([]);
        return forkJoin({
          service: this.api.service(id),
          tenant: this.api.tenant(),
          persons: this.api.persons().pipe(map((p) => p.items)),
          bia: this.api.bia(id),
          microservices: of(microservices),
          dependencies: perMs((m) => this.api.dependencies(m)),
          objectives: perMs((m) => this.api.objectives(m)),
          scenarios: this.api.scenarios(id),
          workflow: this.api.workflow(id),
          readiness: this.api.serviceReadiness(id),
        });
      }),
    );
  }
}

function splitList(value: string): string[] {
  return value
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
}

/** Drops empty strings so optional fields stay unset. */
function clean(value: EditResult): Record<string, string> {
  return Object.fromEntries(
    Object.entries(value)
      .filter(([, v]) => typeof v === 'string' && v.trim())
      .map(([k, v]) => [k, String(v).trim()]),
  );
}
