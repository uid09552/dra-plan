import { Component, computed, inject, input, output } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatTooltipModule } from '@angular/material/tooltip';
import { Observable } from 'rxjs';

import { Api } from '../../../core/api/api';
import { DependencyGraph, GraphNode } from '../../../core/api/models';
import { injectMutation } from '../../../core/http/mutation';
import { TranslatePipe } from '../../../core/i18n/i18n';
import { ContextMenu, MenuItem } from '../../../core/ui/context-menu';
import { ComponentEditor } from './component-editor';

const NODE_W = 170;
const NODE_H = 46;
const COL_GAP = 90;
const ROW_GAP = 22;
const PAD = 16;

interface PlacedNode extends GraphNode {
  x: number;
  y: number;
}

/**
 * Places nodes in columns by dependency depth: things nothing depends on further (foundations)
 * on the left, their dependents to the right, so the map reads in restore order.
 */
export function layoutGraph(graph: DependencyGraph): {
  nodes: PlacedNode[];
  width: number;
  height: number;
} {
  const outgoing = new Map<string, string[]>();
  for (const e of graph.edges) {
    outgoing.set(e.from, [...(outgoing.get(e.from) ?? []), e.to]);
  }
  const depth = new Map<string, number>();
  const visiting = new Set<string>();
  const depthOf = (id: string): number => {
    const known = depth.get(id);
    if (known !== undefined) {
      return known;
    }
    if (visiting.has(id)) {
      return 0; // cycle: reported separately by the backend
    }
    visiting.add(id);
    const d = Math.max(-1, ...(outgoing.get(id) ?? []).map(depthOf)) + 1;
    visiting.delete(id);
    depth.set(id, d);
    return d;
  };
  const columns: GraphNode[][] = [];
  for (const node of graph.nodes) {
    (columns[depthOf(node.id)] ??= []).push(node);
  }
  const nodes: PlacedNode[] = [];
  columns.forEach((column, c) =>
    column.forEach((node, r) =>
      nodes.push({
        ...node,
        x: PAD + c * (NODE_W + COL_GAP),
        y: PAD + r * (NODE_H + ROW_GAP),
      }),
    ),
  );
  const rows = Math.max(1, ...columns.map((c) => c?.length ?? 0));
  return {
    nodes,
    width: PAD * 2 + Math.max(1, columns.length) * (NODE_W + COL_GAP) - COL_GAP,
    height: PAD * 2 + rows * (NODE_H + ROW_GAP) - ROW_GAP,
  };
}

/** Visual dependency map with single points of failure and RTO conflicts highlighted. */
@Component({
  selector: 'app-dependency-map',
  imports: [
    MatButtonModule,
    MatIconModule,
    MatProgressBarModule,
    MatTooltipModule,
    TranslatePipe,
    ContextMenu,
  ],
  template: `
    <div class="d-flex flex-wrap align-items-center gap-2 mb-2">
      <span class="muted small flex-grow-1">{{ 'dependencies.menuHint' | t }}</span>
      <button mat-stroked-button (click)="addComponent()" [disabled]="m.busy()">
        <mat-icon>add</mat-icon>{{ 'guide.components.add' | t }}
      </button>
    </div>
    @for (e of m.issues(); track e) {
      <div class="status error mb-2">{{ e }}</div>
    }
    @if (graph.isLoading()) {
      <mat-progress-bar mode="indeterminate" />
    }
    @if (graph.value(); as g) {
      @if (g.nodes.length === 0) {
        <p class="muted">{{ 'dependencies.empty' | t }}</p>
      } @else {
        <div class="legend d-flex flex-wrap gap-3 mb-2 muted">
          <span><span class="swatch ms"></span>{{ 'dependencies.legend.microservice' | t }}</span>
          <span><span class="swatch ext"></span>{{ 'dependencies.legend.external' | t }}</span>
          <span><span class="swatch spof"></span>{{ 'dependencies.legend.spof' | t }}</span>
          <span><span class="line conflict"></span>{{ 'dependencies.legend.conflict' | t }}</span>
          <span>{{ 'dependencies.legend.direction' | t }}</span>
        </div>
        <div class="map-scroll">
          <svg
            [attr.width]="layout().width"
            [attr.height]="layout().height"
            [attr.viewBox]="'0 0 ' + layout().width + ' ' + layout().height"
            role="group"
            [attr.aria-label]="'dependencies.map' | t"
            (contextmenu)="menu.open($event, backgroundMenu())"
          >
            <defs>
              <marker
                id="dep-arrow"
                viewBox="0 0 10 10"
                refX="10"
                refY="5"
                markerWidth="7"
                markerHeight="7"
                orient="auto-start-reverse"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" class="arrow" />
              </marker>
            </defs>
            @for (e of edges(); track $index) {
              <path
                [attr.d]="e.path"
                class="edge"
                [class.critical]="e.critical"
                [class.conflict]="e.conflict"
                marker-end="url(#dep-arrow)"
              />
            }
            @for (n of layout().nodes; track n.id) {
              <g
                [attr.transform]="'translate(' + n.x + ',' + n.y + ')'"
                [attr.tabindex]="n.type === 'microservice' ? 0 : null"
                [class.editable]="n.type === 'microservice'"
                (contextmenu)="nodeMenu($event, n, menu)"
                (dblclick)="editNode(n)"
              >
                <title>{{ n.label }}{{ n.singlePointOfFailure ? ' — SPOF' : '' }}</title>
                <rect
                  [attr.width]="nodeW"
                  [attr.height]="nodeH"
                  rx="8"
                  class="node"
                  [class.ms]="n.type === 'microservice'"
                  [class.spof]="!!n.singlePointOfFailure"
                />
                <text x="10" y="19" class="label">{{ short(n.label) }}</text>
                <text x="10" y="36" class="meta">
                  {{ 'dependencies.kind.' + n.type | t
                  }}{{ n.rtoMinutes != null ? ' · RTO ' + n.rtoMinutes + ' min' : '' }}
                </text>
                @if (n.singlePointOfFailure) {
                  <text [attr.x]="nodeW - 22" y="19" class="badge">⚠</text>
                }
              </g>
            }
          </svg>
        </div>

        @if (spofs().length) {
          <h3 class="section">{{ 'dependencies.spofTitle' | t }}</h3>
          <ul class="plain">
            @for (n of spofs(); track n.id) {
              <li>
                <mat-icon class="spof-icon">warning</mat-icon>
                <strong>{{ n.label }}</strong> —
                {{ 'dependencies.spof.' + n.singlePointOfFailure | t }}
              </li>
            }
          </ul>
        }
        @if (g.cycles.length) {
          <p class="status error">{{ 'dependencies.cycles' | t: { count: g.cycles.length } }}</p>
        }
      }
    }

    <app-context-menu #menu />
  `,
  styles: `
    .map-scroll {
      overflow: auto;
      border: 1px solid var(--mat-sys-outline-variant);
      border-radius: 12px;
      background: var(--mat-sys-surface-container-lowest);
    }
    svg {
      display: block;
    }
    .node {
      fill: var(--mat-sys-surface-container-high);
      stroke: var(--mat-sys-outline);
      stroke-width: 1;
    }
    .editable {
      cursor: context-menu;
    }
    .small {
      font: var(--mat-sys-body-small);
    }
    .node.ms {
      fill: var(--mat-sys-primary-container);
      stroke: var(--mat-sys-primary);
    }
    .node.spof {
      stroke: var(--mat-sys-error);
      stroke-width: 3;
    }
    .label {
      font:
        500 13px Roboto,
        sans-serif;
      fill: var(--mat-sys-on-surface);
    }
    .meta {
      font:
        11px Roboto,
        sans-serif;
      fill: var(--mat-sys-on-surface-variant);
    }
    .badge {
      font-size: 14px;
      fill: var(--mat-sys-error);
    }
    .edge {
      fill: none;
      stroke: var(--mat-sys-outline);
      stroke-width: 1.5;
    }
    .edge.critical {
      stroke-width: 2.5;
    }
    .edge.conflict {
      stroke: var(--mat-sys-error);
      stroke-dasharray: 6 4;
    }
    .arrow {
      fill: var(--mat-sys-outline);
    }
    .swatch {
      display: inline-block;
      width: 14px;
      height: 14px;
      border-radius: 3px;
      margin-right: 6px;
      vertical-align: -2px;
      border: 1px solid var(--mat-sys-outline);
    }
    .swatch.ms {
      background: var(--mat-sys-primary-container);
    }
    .swatch.ext {
      background: var(--mat-sys-surface-container-high);
    }
    .swatch.spof {
      border: 3px solid var(--mat-sys-error);
    }
    .line {
      display: inline-block;
      width: 24px;
      margin-right: 6px;
      vertical-align: middle;
      border-top: 2px dashed var(--mat-sys-error);
    }
    .section {
      font: var(--mat-sys-title-medium);
      margin: 16px 0 8px;
    }
    .plain {
      list-style: none;
      padding: 0;
    }
    .plain li {
      display: flex;
      align-items: center;
      gap: 6px;
      padding: 2px 0;
    }
    .spof-icon {
      color: var(--mat-sys-error);
    }
  `,
})
export class DependencyMap {
  readonly serviceId = input.required<string>();
  readonly reloadKey = input(0);
  /** Emitted after components or dependencies changed. */
  readonly changed = output<void>();

  private readonly api = inject(Api);
  private readonly editor = inject(ComponentEditor);
  protected readonly m = injectMutation();
  protected readonly components = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey() }),
    stream: ({ params }) => this.api.microservices(params.id),
  });
  protected readonly nodeW = NODE_W;
  protected readonly nodeH = NODE_H;

  protected readonly graph = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey() }),
    stream: ({ params }) => this.api.dependencyGraph(params.id),
  });

  protected readonly layout = computed(() =>
    layoutGraph(
      this.graph.value() ?? { nodes: [], edges: [], suggestedRestoreOrder: [], cycles: [] },
    ),
  );

  protected readonly edges = computed(() => {
    const byId = new Map(this.layout().nodes.map((n) => [n.id, n]));
    return (this.graph.value()?.edges ?? []).flatMap((e) => {
      const from = byId.get(e.from);
      const to = byId.get(e.to);
      if (!from || !to) {
        return [];
      }
      // Dependent (right) → dependency (left).
      const x1 = from.x;
      const y1 = from.y + NODE_H / 2;
      const x2 = to.x + NODE_W;
      const y2 = to.y + NODE_H / 2;
      const mid = (x1 + x2) / 2;
      return [
        {
          path: `M ${x1} ${y1} C ${mid} ${y1}, ${mid} ${y2}, ${x2} ${y2}`,
          critical: e.criticality === 'critical',
          conflict: e.rtoConflict,
        },
      ];
    });
  });

  protected readonly spofs = computed(() =>
    (this.graph.value()?.nodes ?? []).filter((n) => n.singlePointOfFailure),
  );

  protected backgroundMenu(): MenuItem[] {
    return [{ icon: 'add', label: 'guide.components.add', run: () => this.addComponent() }];
  }

  protected nodeMenu(event: Event, node: GraphNode, menu: ContextMenu): void {
    const ms = this.component(node);
    if (!ms) {
      return; // external dependencies are edited in the guide (dependency table)
    }
    menu.open(event, [
      { icon: 'edit', label: 'common.edit', run: () => this.save(this.editor.edit(ms)) },
      {
        icon: 'hub',
        label: 'guide.dependencies.add',
        run: () => this.save(this.editor.addDependency(ms, this.components.value() ?? [])),
      },
      { icon: 'add', label: 'guide.components.add', run: () => this.addComponent() },
      {
        icon: 'delete',
        label: 'common.delete',
        danger: true,
        divider: true,
        disabled: ms.isDefault,
        run: () => this.save(this.editor.remove(ms), 'common.deleted'),
      },
    ]);
  }

  protected editNode(node: GraphNode): void {
    const ms = this.component(node);
    if (ms) {
      this.save(this.editor.edit(ms));
    }
  }

  protected addComponent(): void {
    this.save(this.editor.add(this.serviceId()));
  }

  private component(node: GraphNode) {
    return node.type === 'microservice'
      ? this.components.value()?.find((c) => c.id === node.id)
      : undefined;
  }

  private save(request: Observable<unknown>, success = 'common.saved'): void {
    this.m.run(request, { success, done: () => this.changed.emit() });
  }

  protected short(label: string): string {
    return label.length > 22 ? `${label.slice(0, 21)}…` : label;
  }
}
