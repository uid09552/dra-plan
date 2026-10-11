import {
  Issue,
  Microservice,
  RecoveryObjective,
  Runbook,
  RunbookPhase,
  RunbookStep,
  RunbookStepFields,
  Scenario,
} from '../../../core/api/models';
import { EditResult } from '../../../core/ui/edit-dialog';
import { affectedComponents } from './affected-components';

/** One affected component of a DR scenario with its runbooks (none = missing). */
export interface RunbookRow {
  scenario: Scenario;
  component: Microservice;
  runbooks: Runbook[];
}

export interface ScenarioRunbooks {
  scenario: Scenario;
  rows: RunbookRow[];
}

/** Scenarios that need runbooks: selected with DR required (as in the workflow gate). */
export function isDrScenario(s: Scenario): boolean {
  return s.status === 'selected' && s.drRequired === 'yes';
}

/**
 * Runbooks grouped by DR scenario, one row per affected component (the same pairs the workflow
 * gate checks). Runbooks without a strategy are included.
 */
export function runbookGroups(
  scenarios: Scenario[],
  components: Microservice[],
  runbooks: Runbook[],
): ScenarioRunbooks[] {
  return scenarios.filter(isDrScenario).map((scenario) => ({
    scenario,
    rows: affectedComponents(scenario, components).map((component) => ({
      scenario,
      component,
      runbooks: runbooks.filter(
        (r) => r.scenarioId === scenario.id && r.microserviceId === component.id,
      ),
    })),
  }));
}

/** A step in execution order; `phaseStart` marks where a phase separator is drawn above it. */
export interface StepRow {
  step: RunbookStep;
  phaseStart?: RunbookPhase;
}

export function stepRows(steps: RunbookStep[]): StepRow[] {
  const sorted = [...steps].sort((a, b) => a.seq - b.seq);
  return sorted.map((step, i) =>
    i === 0 || sorted[i - 1].phase !== step.phase ? { step, phaseStart: step.phase } : { step },
  );
}

/** Steps a step at `seq` may depend on: only earlier ones. */
export function dependencyOptions(steps: RunbookStep[], seq: number): RunbookStep[] {
  return steps.filter((s) => s.seq < seq).sort((a, b) => a.seq - b.seq);
}

/** Step ids in execution order after moving the step at `from` to `to` (0-based). */
export function movedOrder(steps: RunbookStep[], from: number, to: number): string[] {
  const ids = [...steps].sort((a, b) => a.seq - b.seq).map((s) => s.id);
  const [id] = ids.splice(from, 1);
  ids.splice(Math.max(0, Math.min(to, ids.length)), 0, id);
  return ids;
}

/** Effective RTO of a component for a scenario: scenario-specific, else the component default. */
export function effectiveRto(
  objectives: RecoveryObjective[],
  microserviceId: string,
  scenarioId: string,
): number | undefined {
  const own = objectives.filter((o) => o.microserviceId === microserviceId);
  return (own.find((o) => o.scenarioId === scenarioId) ?? own.find((o) => !o.scenarioId))
    ?.rtoMinutes;
}

/** Gate issues of the given entity (`runbook` or `runbook_step`). */
export function issuesOf(issues: Issue[], entityType: string, entityId: string): Issue[] {
  return issues.filter((i) => i.entityType === entityType && i.entityId === entityId);
}

/** Maps the step dialog's form value to the API fields (blank texts clear a field). */
export function stepBody(v: EditResult): RunbookStepFields {
  const text = (key: string) => String(v[key] ?? '').trim();
  const id = (key: string) => (v[key] as string | null) || null;
  const minutes = v['expectedDurationMinutes'];
  return {
    phase: v['phase'] as RunbookPhase,
    title: text('title'),
    instructions: text('instructions'),
    ownerRoleId: id('ownerRoleId'),
    expectedDurationMinutes:
      minutes === null || minutes === '' || minutes === undefined ? null : Number(minutes),
    verification: text('verification'),
    prerequisites: text('prerequisites'),
    evidenceRequired: text('evidenceRequired'),
    dependsOnStepIds: (v['dependsOnStepIds'] as string[] | null) ?? [],
    isDecisionPoint: !!v['isDecisionPoint'],
    requiresAuthorizationRoleId: id('requiresAuthorizationRoleId'),
  };
}
