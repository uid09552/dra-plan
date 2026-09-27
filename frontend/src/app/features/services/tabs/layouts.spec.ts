import { Scenario, WorkflowState } from '../../../core/api/models';
import { layoutGraph } from './dependency-map';
import { riskBand } from './risk-matrix';
import { buildMindTree, layoutMindTree } from './scenario-map';
import { stageState } from './service-guide';

function scenario(id: string, extra: Partial<Scenario> = {}): Scenario {
  return { id, title: id, status: 'brainstormed', serviceId: 's', ...extra } as Scenario;
}

describe('buildMindTree', () => {
  it('groups by category, nests sub-scenarios and hides merged ones', () => {
    const tree = buildMindTree(
      'Shop',
      [
        scenario('a', { category: 'cloud' }),
        scenario('b', { category: 'cloud', parentScenarioId: 'a' }),
        scenario('c', { category: 'database' }),
        scenario('m', { category: 'cloud', status: 'merged' }),
      ],
      (k) => k.toUpperCase(),
    );
    expect(tree.children.map((c) => c.label)).toEqual(['CLOUD', 'DATABASE']);
    expect(tree.children[0].children.map((s) => s.id)).toEqual(['a']);
    expect(tree.children[0].children[0].children.map((s) => s.id)).toEqual(['b']);
  });

  it('places leaves on separate rows and parents between their children', () => {
    const layout = layoutMindTree(
      buildMindTree(
        'Shop',
        [scenario('a', { category: 'cloud' }), scenario('b', { category: 'cloud' })],
        (k) => k,
      ),
    );
    const byId = new Map(layout.nodes.map((n) => [n.id, n]));
    expect(byId.get('a')!.y).not.toEqual(byId.get('b')!.y);
    expect(byId.get('category:cloud')!.y).toEqual((byId.get('a')!.y + byId.get('b')!.y) / 2);
  });
});

describe('layoutGraph', () => {
  it('puts dependencies left of their dependents and survives cycles', () => {
    const { nodes } = layoutGraph({
      nodes: [
        { id: 'app', type: 'microservice', label: 'app' },
        { id: 'db', type: 'microservice', label: 'db' },
        { id: 'x', type: 'microservice', label: 'x' },
      ],
      edges: [
        { from: 'app', to: 'db', criticality: 'critical', rtoConflict: false },
        { from: 'x', to: 'x', criticality: 'critical', rtoConflict: false },
      ],
      suggestedRestoreOrder: [],
      cycles: [],
    });
    const x = (id: string) => nodes.find((n) => n.id === id)!.x;
    expect(x('db')).toBeLessThan(x('app'));
  });
});

describe('riskBand', () => {
  it('maps scores to bands', () => {
    expect([1, 4, 8, 16].map(riskBand)).toEqual(['low', 'medium', 'high', 'critical']);
  });
});

describe('stageState', () => {
  const workflow = (statuses: string[], blocking = false) =>
    ({
      steps: statuses.map((status, i) => ({
        key: `s${i}`,
        status,
        issues: blocking ? [{ severity: 'blocking', ruleId: 'X', message: 'x' }] : [],
      })),
    }) as unknown as WorkflowState;

  it('is done when all steps are complete, blocked with blocking issues', () => {
    expect(stageState(['s0', 's1'], workflow(['complete', 'complete']))).toBe('done');
    expect(stageState(['s0'], workflow(['in_progress'], true))).toBe('blocked');
    expect(stageState(['s0'], workflow(['in_progress']))).toBe('open');
  });
});
