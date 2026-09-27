import {
  Component,
  ElementRef,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { FormBuilder, ReactiveFormsModule, Validators } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatButtonToggleModule } from '@angular/material/button-toggle';
import { MatCardModule } from '@angular/material/card';
import { MatDialog } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatSelectModule } from '@angular/material/select';
import { MatTooltipModule } from '@angular/material/tooltip';
import { Observable } from 'rxjs';

import { Api } from '../../../core/api/api';
import { RecoveryStrategy, Scenario } from '../../../core/api/models';
import { injectMutation } from '../../../core/http/mutation';
import { I18n, TranslatePipe } from '../../../core/i18n/i18n';
import { ContextMenu, MenuItem } from '../../../core/ui/context-menu';
import {
  EditDialogData,
  EditResult,
  FieldDef,
  openConfirm,
  openEdit,
} from '../../../core/ui/edit-dialog';
import { MeasureContext, MeasureEditor } from './measure-editor';
import { RiskMatrix } from './risk-matrix';

const NODE_W = 240;
const NODE_H = 46;
/** Room right of each node for its action buttons. */
const ACTIONS_W = 110;
const COL_W = NODE_W + ACTIONS_W;
const ROW_H = 68;
const PAD = 20;
const ZOOM_MIN = 0.5;
const ZOOM_MAX = 2;

/** Action button shown next to a mind-map node. */
export interface NodeAction {
  icon: string;
  /** i18n key (tooltip and aria-label). */
  label: string;
  run: () => void;
  danger?: boolean;
}

const STATUS_TONE: Record<Scenario['status'], string> = {
  selected: 'ok',
  brainstormed: 'warn',
  rejected: '',
  merged: '',
};

export interface MindNode {
  id: string;
  label: string;
  kind: 'root' | 'category' | 'scenario';
  scenario?: Scenario;
  children: MindNode[];
}

export interface PlacedMindNode extends MindNode {
  x: number;
  y: number;
}

/** Service → categories → scenarios → sub-scenarios (merged scenarios are left out). */
export function buildMindTree(
  serviceName: string,
  scenarios: Scenario[],
  categoryLabel: (key: string) => string,
): MindNode {
  const active = scenarios.filter((s) => s.status !== 'merged');
  const ids = new Set(active.map((s) => s.id));
  const childrenOf = (parent: string): MindNode[] =>
    active
      .filter((s) => s.parentScenarioId === parent)
      .map((s) => ({
        id: s.id,
        label: s.title,
        kind: 'scenario' as const,
        scenario: s,
        children: childrenOf(s.id),
      }));
  const roots = active.filter((s) => !s.parentScenarioId || !ids.has(s.parentScenarioId));
  const categories = [...new Set(roots.map((s) => s.category ?? ''))].sort();
  return {
    id: 'root',
    label: serviceName,
    kind: 'root',
    children: categories.map((c) => ({
      id: `category:${c}`,
      label: c ? categoryLabel(c) : '–',
      kind: 'category',
      children: roots
        .filter((s) => (s.category ?? '') === c)
        .map((s) => ({
          id: s.id,
          label: s.title,
          kind: 'scenario',
          scenario: s,
          children: childrenOf(s.id),
        })),
    })),
  };
}

/** Left-to-right tree layout: leaves get consecutive rows, parents are centered on children. */
export function layoutMindTree(root: MindNode): {
  nodes: PlacedMindNode[];
  links: { from: PlacedMindNode; to: PlacedMindNode }[];
  width: number;
  height: number;
} {
  const nodes: PlacedMindNode[] = [];
  const links: { from: PlacedMindNode; to: PlacedMindNode }[] = [];
  let row = 0;
  let maxDepth = 0;
  const place = (node: MindNode, depth: number): PlacedMindNode => {
    maxDepth = Math.max(maxDepth, depth);
    const children = node.children.map((c) => place(c, depth + 1));
    const y = children.length
      ? (children[0].y + children[children.length - 1].y) / 2
      : PAD + row++ * ROW_H;
    const placed: PlacedMindNode = { ...node, x: PAD + depth * COL_W, y };
    nodes.push(placed);
    children.forEach((c) => links.push({ from: placed, to: c }));
    return placed;
  };
  place(root, 0);
  return {
    nodes,
    links,
    width: PAD * 2 + maxDepth * COL_W + NODE_W + ACTIONS_W,
    height: PAD * 2 + Math.max(1, row) * ROW_H - (ROW_H - NODE_H),
  };
}

/** Scenario brainstorming as a mind map with suggestions, decisions and the risk matrix. */
@Component({
  selector: 'app-scenario-map',
  imports: [
    ReactiveFormsModule,
    MatButtonModule,
    MatButtonToggleModule,
    MatCardModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatSelectModule,
    MatTooltipModule,
    TranslatePipe,
    RiskMatrix,
    ContextMenu,
  ],
  templateUrl: './scenario-map.html',
  styleUrl: './scenario-map.scss',
})
export class ScenarioMap {
  readonly serviceId = input.required<string>();
  readonly serviceName = input('');
  readonly reloadKey = input(0);
  /** Emitted after every successful change. */
  readonly changed = output<void>();

  private readonly api = inject(Api);
  private readonly i18n = inject(I18n);
  private readonly fb = inject(FormBuilder).nonNullable;
  private readonly dialog = inject(MatDialog);
  private readonly measureEditor = inject(MeasureEditor);
  protected readonly m = injectMutation();

  protected readonly scenarios = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey() }),
    stream: ({ params }) => this.api.scenarios(params.id),
  });
  protected readonly suggestions = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey(), lang: this.i18n.lang() }),
    stream: ({ params }) => this.api.scenarioSuggestions(params.id),
  });
  protected readonly catalog = rxResource({
    params: () => this.i18n.lang(),
    stream: () => this.api.catalog(),
  });

  /** Components for "affected components" (only offered when explicit components exist). */
  private readonly components = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey() }),
    stream: ({ params }) => this.api.microservices(params.id),
  });
  /** Measures of the service, to show which ones a scenario is covered by. */
  protected readonly measures = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey() }),
    stream: ({ params }) => this.api.serviceStrategies(params.id),
  });
  protected readonly selectedMeasures = computed(() => {
    const id = this.selectedId();
    return id ? (this.measures.value() ?? []).filter((m) => m.scenarioIds.includes(id)) : [];
  });

  protected readonly nodeW = NODE_W;
  protected readonly nodeH = NODE_H;
  protected readonly zoom = signal(1);
  /** Node under the pointer or keyboard focus; its action buttons are shown. */
  protected readonly hovered = signal<string | null>(null);
  private readonly mapScroll = viewChild<ElementRef<HTMLElement>>('mapScroll');
  private readonly detail = viewChild<ElementRef<HTMLElement>>('detail');
  /** The node whose action buttons are shown: the hovered one, otherwise the selected one. */
  protected readonly actionNode = computed(() => {
    const id = this.hovered() ?? this.selectedId();
    return this.layout().nodes.find((n) => n.id === id);
  });

  protected readonly view = signal<'map' | 'matrix'>('map');
  protected readonly selectedId = signal<string | null>(null);
  protected readonly selected = computed(
    () => this.scenarios.value()?.find((s) => s.id === this.selectedId()) ?? null,
  );
  /** Other non-merged scenarios, for "parent" and "merge into". */
  protected readonly others = computed(() =>
    (this.scenarios.value() ?? []).filter(
      (s) => s.id !== this.selectedId() && s.status !== 'merged',
    ),
  );

  private readonly categoryLabels = computed(
    () => new Map((this.catalog.value()?.scenarioCategories ?? []).map((c) => [c.key, c.label])),
  );
  protected readonly layout = computed(() =>
    layoutMindTree(
      buildMindTree(this.serviceName(), this.scenarios.value() ?? [], (k) => this.categoryLabel(k)),
    ),
  );
  /** Drag state of a mind-map node (pointer events, so it works with touch too). */
  protected readonly drag = signal<{
    id: string;
    label: string;
    startX: number;
    startY: number;
    x: number;
    y: number;
    moved: boolean;
    target: PlacedMindNode | null;
  } | null>(null);
  /** Suppresses the click that follows a drop. */
  private dropped = false;

  protected readonly counts = computed(() => {
    const list = (this.scenarios.value() ?? []).filter((s) => s.status !== 'merged');
    return {
      total: list.length,
      selected: list.filter((s) => s.status === 'selected').length,
      open: list.filter((s) => s.status === 'brainstormed').length,
    };
  });

  protected readonly addForm = this.fb.group({
    title: ['', Validators.required],
    category: [''],
  });
  protected readonly editForm = this.fb.group({
    title: ['', Validators.required],
    category: [''],
    parent: [''],
  });
  protected readonly childForm = this.fb.group({ title: ['', Validators.required] });
  protected readonly decisionForm = this.fb.group({
    likelihood: [2, Validators.required],
    impact: [2, Validators.required],
    drRequired: ['yes'],
    rationale: ['', Validators.required],
  });
  protected readonly mergeForm = this.fb.group({ target: ['', Validators.required] });

  protected readonly drRequiredOptions = [
    'yes',
    'degraded_mode',
    'no_handled_by_ha',
    'no_accepted_risk',
  ];
  protected readonly ratings = [1, 2, 3, 4];

  constructor() {
    // Fill the edit and decision forms when another scenario is picked.
    effect(() => {
      const s = this.selected();
      if (!s) {
        return;
      }
      this.editForm.reset({
        title: s.title,
        category: s.category ?? '',
        parent: s.parentScenarioId ?? '',
      });
      this.decisionForm.reset({
        likelihood: s.likelihood ?? 2,
        impact: s.impact ?? 2,
        drRequired: s.drRequired ?? 'yes',
        rationale: s.decisionRationale ?? '',
      });
      this.childForm.reset();
      this.mergeForm.reset();
    });
  }

  protected tone(s: Scenario): string {
    return STATUS_TONE[s.status];
  }

  protected categoryLabel(key: string): string {
    return this.categoryLabels().get(key) ?? key;
  }

  protected zoomBy(factor: number): void {
    this.zoom.update((z) =>
      Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(z * factor * 10) / 10)),
    );
  }

  /** Fits the whole map into the visible width (never above 100 %). */
  protected fit(): void {
    const width = this.mapScroll()?.nativeElement.clientWidth ?? 0;
    if (width) {
      this.zoom.set(Math.max(ZOOM_MIN, Math.min(1, (width - 4) / this.layout().width)));
    }
  }

  protected addScenario(): void {
    this.addDialog(null, null);
  }

  /** Buttons next to a node: the same actions as its right-click menu. */
  protected nodeActions(n: PlacedMindNode): NodeAction[] {
    if (n.kind === 'root') {
      return [
        { icon: 'add', label: 'scenarioMap.newScenario', run: () => this.addDialog(null, null) },
      ];
    }
    if (n.kind === 'category') {
      const category = n.id.slice('category:'.length) || null;
      return [
        {
          icon: 'add',
          label: 'scenarioMap.addInCategory',
          run: () => this.addDialog(category, null),
        },
      ];
    }
    const s = n.scenario;
    if (!s) {
      return [];
    }
    return [
      {
        icon: 'subdirectory_arrow_right',
        label: 'scenarioMap.subScenario',
        run: () => this.addDialog(s.category ?? null, s.id),
      },
      { icon: 'edit', label: 'common.edit', run: () => this.editDialog(s) },
      { icon: 'rule', label: 'scenarioMap.decide', run: () => this.openDetail(s.id) },
      { icon: 'delete', label: 'common.delete', danger: true, run: () => this.remove(s) },
    ];
  }

  /** Selects a scenario and scrolls its detail card (decision, measures) into view. */
  protected openDetail(id: string): void {
    this.selectedId.set(id);
    setTimeout(() =>
      this.detail()?.nativeElement.scrollIntoView({ behavior: 'smooth', block: 'start' }),
    );
  }

  protected short(label: string, max = 24): string {
    return label.length > max ? `${label.slice(0, max - 1)}…` : label;
  }

  protected pick(node: PlacedMindNode | Scenario): void {
    if (this.dropped) {
      this.dropped = false;
      return;
    }
    const scenario = 'kind' in node ? node.scenario : node;
    if (scenario) {
      this.selectedId.set(scenario.id);
    }
  }

  protected linkPath(from: PlacedMindNode, to: PlacedMindNode): string {
    const x1 = from.x + NODE_W;
    const y1 = from.y + NODE_H / 2;
    const x2 = to.x;
    const y2 = to.y + NODE_H / 2;
    const mid = (x1 + x2) / 2;
    return `M ${x1} ${y1} C ${mid} ${y1}, ${mid} ${y2}, ${x2} ${y2}`;
  }

  protected accept(templateId: string): void {
    this.m.run(this.api.createScenario(this.serviceId(), { catalogTemplateId: templateId }), {
      success: 'scenarioMap.added',
      done: (s) => this.afterChange(s.id),
    });
  }

  protected add(): void {
    const v = this.addForm.getRawValue();
    this.m.run(
      this.api.createScenario(this.serviceId(), {
        title: v.title.trim(),
        category: v.category || undefined,
      }),
      {
        success: 'scenarioMap.added',
        done: (s) => {
          this.addForm.reset();
          this.afterChange(s.id);
        },
      },
    );
  }

  protected addChild(parent: Scenario): void {
    this.m.run(
      this.api.createScenario(this.serviceId(), {
        title: this.childForm.getRawValue().title.trim(),
        category: parent.category,
        parentScenarioId: parent.id,
      }),
      { success: 'scenarioMap.added', done: (s) => this.afterChange(s.id) },
    );
  }

  protected save(s: Scenario): void {
    const v = this.editForm.getRawValue();
    this.m.run(
      this.api.updateScenario(s.id, {
        title: v.title.trim(),
        category: v.category || null,
        parentScenarioId: v.parent || null,
      }),
      { success: 'common.saved', done: () => this.afterChange(s.id) },
    );
  }

  protected decide(s: Scenario, decision: 'selected' | 'rejected'): void {
    const v = this.decisionForm.getRawValue();
    this.m.run(
      this.api.decideScenario(s.id, {
        decision,
        decisionRationale: v.rationale.trim(),
        likelihood: v.likelihood,
        impact: v.impact,
        drRequired: decision === 'selected' ? v.drRequired : undefined,
      }),
      {
        success: decision === 'selected' ? 'scenarioMap.accepted' : 'scenarioMap.rejected',
        done: () => this.afterChange(s.id),
      },
    );
  }

  protected merge(s: Scenario): void {
    const target = this.mergeForm.getRawValue().target;
    this.m.run(this.api.mergeScenarios(target, [s.id]), {
      success: 'scenarioMap.merged',
      done: () => this.afterChange(target),
    });
  }

  // ── Context menus ──
  protected nodeMenu(event: Event, node: PlacedMindNode, menu: ContextMenu): void {
    if (node.kind === 'root') {
      menu.open(event, [
        { icon: 'add', label: 'scenarioMap.newScenario', run: () => this.addDialog(null, null) },
      ]);
    } else if (node.kind === 'category') {
      const category = node.id.slice('category:'.length) || null;
      menu.open(event, [
        {
          icon: 'add',
          label: 'scenarioMap.addInCategory',
          run: () => this.addDialog(category, null),
        },
      ]);
    } else if (node.scenario) {
      menu.open(event, this.scenarioMenu(node.scenario));
    }
  }

  /** Number of measures covering a scenario (badge in the mind map). */
  protected measureCount(scenarioId: string): number {
    return (this.measures.value() ?? []).filter((m) => m.scenarioIds.includes(scenarioId)).length;
  }

  protected measureLabel(st: RecoveryStrategy): string {
    return this.measureEditor.label(st, this.catalog.value());
  }

  protected componentName(id: string): string {
    return this.components.value()?.find((c) => c.id === id)?.name ?? '–';
  }

  protected addMeasure(s: Scenario): void {
    this.m.run(this.measureEditor.add(this.measureContext(), { scenarioIds: [s.id] }), {
      success: 'common.saved',
      done: () => this.afterChange(s.id),
    });
  }

  protected measureMenu(st: RecoveryStrategy): MenuItem[] {
    const selected = this.selectedId() ?? '';
    return [
      {
        icon: 'edit',
        label: 'common.edit',
        run: () =>
          this.m.run(this.measureEditor.edit(st, this.measureContext()), {
            success: 'common.saved',
            done: () => this.afterChange(selected),
          }),
      },
      {
        icon: 'check_circle',
        label: 'guide.mitigations.select',
        disabled: st.isSelected,
        run: () =>
          this.m.run(this.measureEditor.select(st), {
            success: 'guide.mitigations.selected',
            done: () => this.afterChange(selected),
          }),
      },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        run: () =>
          this.m.run(this.measureEditor.remove(st, this.measureLabel(st)), {
            success: 'common.deleted',
            done: () => this.afterChange(selected),
          }),
      },
    ];
  }

  private measureContext(): MeasureContext {
    return {
      components: this.components.value() ?? [],
      scenarios: this.scenarios.value() ?? [],
      catalog: this.catalog.value(),
    };
  }

  protected scenarioMenu(s: Scenario): MenuItem[] {
    return [
      { icon: 'edit', label: 'common.edit', run: () => this.editDialog(s) },
      { icon: 'shield', label: 'measures.add', run: () => this.addMeasure(s) },
      {
        icon: 'subdirectory_arrow_right',
        label: 'scenarioMap.subScenario',
        run: () => this.addDialog(s.category ?? null, s.id),
      },
      {
        icon: 'rule',
        label: 'scenarioMap.decide',
        run: () => this.selectedId.set(s.id),
      },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        run: () => this.remove(s),
      },
    ];
  }

  protected remove(s: Scenario): void {
    openConfirm(this.dialog, {
      title: 'scenarioMap.deleteConfirm',
      message: s.title,
      confirm: 'common.delete',
    }).subscribe((ok) => {
      if (ok) {
        this.m.run(this.api.deleteScenario(s.id), {
          success: 'common.deleted',
          done: () => {
            if (this.selectedId() === s.id) {
              this.selectedId.set(null);
            }
            this.changed.emit();
          },
        });
      }
    });
  }

  private categoryOptions() {
    return [
      { value: null, label: this.i18n.t('common.none') },
      ...(this.catalog.value()?.scenarioCategories ?? []).map((c) => ({
        value: c.key,
        label: c.label,
      })),
    ];
  }

  /** Multi-select of affected components; empty = whole service. Hidden without explicit components. */
  private affectedField(): FieldDef[] {
    const explicit = (this.components.value() ?? []).filter((c) => !c.isDefault);
    if (!explicit.length) {
      return [];
    }
    return [
      {
        key: 'affected',
        label: 'scenarioMap.affected',
        type: 'multiselect',
        hint: this.i18n.t('scenarioMap.affectedHint'),
        options: explicit.map((c) => ({ value: c.id, label: c.name })),
      },
    ];
  }

  private addDialog(category: string | null, parentId: string | null): void {
    this.dialogThen(
      {
        title: parentId ? 'scenarioMap.subScenario' : 'scenarioMap.newScenario',
        submit: 'common.add',
        fields: [
          { key: 'title', label: 'common.name', type: 'text', required: true },
          { key: 'description', label: 'common.description', type: 'textarea' },
          {
            key: 'category',
            label: 'scenarios.category',
            type: 'select',
            options: this.categoryOptions(),
          },
          ...this.affectedField(),
        ],
        value: { category },
      },
      (v) =>
        this.api.createScenario(this.serviceId(), {
          affectedMicroserviceIds: (v['affected'] as string[] | undefined) ?? [],
          title: String(v['title']).trim(),
          category: (v['category'] as string | null) ?? undefined,
          parentScenarioId: parentId ?? undefined,
          ...(v['description'] ? { description: String(v['description']).trim() } : {}),
        }),
      'scenarioMap.added',
    );
  }

  private editDialog(s: Scenario): void {
    this.dialogThen(
      {
        title: 'scenarioMap.edit',
        fields: [
          { key: 'title', label: 'common.name', type: 'text', required: true },
          { key: 'description', label: 'common.description', type: 'textarea' },
          {
            key: 'category',
            label: 'scenarios.category',
            type: 'select',
            options: this.categoryOptions(),
          },
          ...this.affectedField(),
        ],
        value: {
          title: s.title,
          description: s.description ?? '',
          category: s.category ?? null,
          affected: s.affectedMicroserviceIds ?? [],
        },
      },
      (v) =>
        this.api.updateScenario(s.id, {
          ...(v['affected'] ? { affectedMicroserviceIds: v['affected'] as string[] } : {}),
          title: String(v['title']).trim(),
          description: String(v['description'] ?? '').trim(),
          category: (v['category'] as string | null) ?? null,
        }),
      'common.saved',
    );
  }

  private dialogThen(
    data: EditDialogData,
    save: (v: EditResult) => Observable<Scenario>,
    success: string,
  ): void {
    openEdit(this.dialog, data).subscribe((v) => {
      if (v) {
        this.m.run(save(v), { success, done: (s) => this.afterChange(s.id) });
      }
    });
  }

  // ── Drag & drop in the mind map ──
  protected dragStart(event: PointerEvent, node: PlacedMindNode): void {
    if (!node.scenario || event.button !== 0) {
      return;
    }
    const svg = (event.currentTarget as Element).closest('svg');
    svg?.setPointerCapture(event.pointerId);
    const { x, y } = this.svgPoint(event, svg);
    this.drag.set({
      id: node.id,
      label: node.label,
      startX: x,
      startY: y,
      x,
      y,
      moved: false,
      target: null,
    });
  }

  protected dragMove(event: PointerEvent): void {
    const d = this.drag();
    if (!d) {
      return;
    }
    const { x, y } = this.svgPoint(event, event.currentTarget as SVGSVGElement);
    const moved = d.moved || Math.hypot(x - d.startX, y - d.startY) > 6;
    const target =
      this.layout().nodes.find(
        (n) => n.id !== d.id && x >= n.x && x <= n.x + NODE_W && y >= n.y && y <= n.y + NODE_H,
      ) ?? null;
    this.drag.set({ ...d, x, y, moved, target });
  }

  protected dragEnd(): void {
    const d = this.drag();
    this.drag.set(null);
    if (!d) {
      return;
    }
    if (!d.moved) {
      // Pointer capture on the SVG swallows the node's click event, so select here.
      this.selectedId.set(d.id);
      return;
    }
    this.dropped = true;
    const scenario = this.scenarios.value()?.find((s) => s.id === d.id);
    if (scenario && d.target) {
      this.moveTo(scenario, d.target);
    }
  }

  /** Dropping on a scenario makes it the parent, on a category moves it there, on the service detaches it. */
  private moveTo(s: Scenario, target: PlacedMindNode): void {
    let patch: { category?: string | null; parentScenarioId: string | null };
    if (target.kind === 'scenario') {
      if (s.parentScenarioId === target.id) {
        return;
      }
      patch = { parentScenarioId: target.id };
    } else if (target.kind === 'category') {
      patch = { category: target.id.slice('category:'.length) || null, parentScenarioId: null };
    } else {
      patch = { parentScenarioId: null };
    }
    this.m.run(this.api.updateScenario(s.id, patch), {
      success: 'scenarioMap.moved',
      done: () => this.afterChange(s.id),
    });
  }

  /** Risk matrix drop: sets likelihood and impact. */
  protected rate(event: { scenario: Scenario; likelihood: number; impact: number }): void {
    const { scenario, likelihood, impact } = event;
    if (scenario.likelihood === likelihood && scenario.impact === impact) {
      return;
    }
    this.m.run(this.api.updateScenario(scenario.id, { likelihood, impact }), {
      success: 'scenarioMap.rated',
      done: () => this.afterChange(scenario.id),
    });
  }

  private svgPoint(event: PointerEvent, svg: Element | null): { x: number; y: number } {
    const rect = svg?.getBoundingClientRect();
    const zoom = this.zoom();
    return {
      x: (event.clientX - (rect?.left ?? 0)) / zoom,
      y: (event.clientY - (rect?.top ?? 0)) / zoom,
    };
  }

  private afterChange(selectId: string): void {
    this.selectedId.set(selectId);
    this.changed.emit();
  }
}
