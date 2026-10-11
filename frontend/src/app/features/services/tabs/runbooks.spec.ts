import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { BehaviorSubject } from 'rxjs';

import { API_BASE } from '../../../core/api/api';
import {
  Issue,
  Microservice,
  Runbook,
  RunbookDetail,
  RunbookStep,
  Scenario,
} from '../../../core/api/models';
import { I18n } from '../../../core/i18n/i18n';
import { RunbookEditor } from './runbook-editor';
import {
  dependencyOptions,
  effectiveRto,
  movedOrder,
  runbookGroups,
  stepBody,
  stepRows,
} from './runbook-model';
import { RunbooksView } from './runbooks-view';

const meta = {
  tenantId: 't',
  version: 1,
  createdAt: '',
  createdBy: '',
  updatedAt: '',
  updatedBy: '',
};

function scenario(id: string, extra: Partial<Scenario> = {}): Scenario {
  return {
    id,
    title: id,
    status: 'selected',
    drRequired: 'yes',
    serviceId: 's',
    affectedMicroserviceIds: [],
    ...extra,
  } as Scenario;
}

function component(id: string, isDefault = false): Microservice {
  return { ...meta, id, name: id, serviceId: 's', dataStores: [], isDefault } as Microservice;
}

function runbook(id: string, scenarioId: string, microserviceId: string): Runbook {
  return {
    ...meta,
    id,
    scenarioId,
    microserviceId,
    title: `Runbook ${id}`,
    stepCount: 0,
    criticalPathMinutes: 0,
    exceedsRto: false,
  };
}

function step(
  id: string,
  seq: number,
  phase: RunbookStep['phase'] = 'recovery',
  extra: Partial<RunbookStep> = {},
): RunbookStep {
  return {
    ...meta,
    id,
    seq,
    phase,
    title: `Step ${id}`,
    runbookId: 'rb',
    dependsOnStepIds: [],
    isDecisionPoint: false,
    ...extra,
  };
}

describe('runbookGroups', () => {
  it('lists every affected component and marks missing runbooks', () => {
    const groups = runbookGroups(
      [
        scenario('ransomware', { affectedMicroserviceIds: ['db', 'app'] }),
        scenario('x', { drRequired: 'no' }),
      ],
      [component('db'), component('app'), component('whole', true)],
      [runbook('r1', 'ransomware', 'db')],
    );
    expect(groups.map((g) => g.scenario.id)).toEqual(['ransomware']);
    expect(groups[0].rows.map((r) => [r.component.id, r.runbooks.length])).toEqual([
      ['db', 1],
      ['app', 0],
    ]);
  });

  it('falls back to the default component and keeps runbooks without a strategy', () => {
    const groups = runbookGroups(
      [scenario('outage')],
      [component('db'), component('whole', true)],
      [{ ...runbook('r1', 'outage', 'whole'), strategyId: null }],
    );
    expect(groups[0].rows.map((r) => r.component.id)).toEqual(['whole']);
    expect(groups[0].rows[0].runbooks.map((r) => r.id)).toEqual(['r1']);
  });
});

describe('step helpers', () => {
  it('draws a phase separator wherever the phase changes', () => {
    const rows = stepRows([
      step('c', 3, 'reconstitution'),
      step('a', 1, 'activation'),
      step('b', 2, 'recovery'),
      step('d', 4, 'reconstitution'),
    ]);
    expect(rows.map((r) => [r.step.id, r.phaseStart])).toEqual([
      ['a', 'activation'],
      ['b', 'recovery'],
      ['c', 'reconstitution'],
      ['d', undefined],
    ]);
  });

  it('offers only earlier steps as dependencies', () => {
    const steps = [step('a', 1), step('b', 2), step('c', 3)];
    expect(dependencyOptions(steps, 3).map((s) => s.id)).toEqual(['a', 'b']);
    expect(dependencyOptions(steps, 1)).toEqual([]);
  });

  it('moves a step to a new position', () => {
    const steps = [step('a', 1), step('b', 2), step('c', 3)];
    expect(movedOrder(steps, 2, 0)).toEqual(['c', 'a', 'b']);
    expect(movedOrder(steps, 0, 1)).toEqual(['b', 'a', 'c']);
  });

  it('maps the dialog value and clears blank texts', () => {
    const body = stepBody({
      phase: 'recovery',
      title: ' Restore ',
      instructions: '',
      ownerRoleId: null,
      expectedDurationMinutes: '',
      verification: 'db readable',
      prerequisites: ' Vault access ',
      evidenceRequired: '  ',
      dependsOnStepIds: ['a'],
      isDecisionPoint: false,
      requiresAuthorizationRoleId: 'ciso',
    });
    expect(body).toMatchObject({
      title: 'Restore',
      ownerRoleId: null,
      expectedDurationMinutes: null,
      prerequisites: 'Vault access',
      evidenceRequired: '',
      requiresAuthorizationRoleId: 'ciso',
    });
  });

  it('prefers the scenario-specific RTO', () => {
    const objectives = [
      { ...meta, id: '1', microserviceId: 'db', rtoMinutes: 240, rpoMinutes: 60 },
      { ...meta, id: '2', microserviceId: 'db', scenarioId: 's1', rtoMinutes: 60, rpoMinutes: 5 },
    ];
    expect(effectiveRto(objectives, 'db', 's1')).toBe(60);
    expect(effectiveRto(objectives, 'db', 's2')).toBe(240);
    expect(effectiveRto(objectives, 'app', 's1')).toBeUndefined();
  });
});

function setup(queryParams: Record<string, string> = {}) {
  const params = new BehaviorSubject(convertToParamMap(queryParams));
  TestBed.configureTestingModule({
    providers: [
      provideHttpClient(),
      provideHttpClientTesting(),
      provideRouter([]),
      {
        provide: ActivatedRoute,
        useValue: { queryParamMap: params, snapshot: { queryParamMap: params.value } },
      },
    ],
  });
  TestBed.inject(I18n).use('en');
  return TestBed.inject(HttpTestingController);
}

/** Waits until nothing is pending (only call when no HTTP request is outstanding). */
async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  await fixture.whenStable();
  fixture.detectChanges();
}

/** Runs effects and change detection so resources issue their requests. */
function tick(fixture: ComponentFixture<unknown>): void {
  TestBed.tick();
  fixture.detectChanges();
}

function menuLabels(): string[] {
  return [...document.querySelectorAll('.mat-mdc-menu-item')].map((e) => e.textContent!.trim());
}

describe('RunbooksView', () => {
  afterEach(() => document.querySelector('.cdk-overlay-container')?.replaceChildren());

  async function render(http: HttpTestingController, queryRunbook = false) {
    const fixture = TestBed.createComponent(RunbooksView);
    fixture.componentRef.setInput('serviceId', 's');
    tick(fixture);
    http.expectOne(`${API_BASE}/services/s/runbooks`).flush([runbook('r1', 'ransomware', 'db')]);
    http
      .expectOne(`${API_BASE}/services/s/microservices`)
      .flush([component('db'), component('app')]);
    http
      .expectOne(`${API_BASE}/services/s/scenarios`)
      .flush([scenario('ransomware', { affectedMicroserviceIds: ['db', 'app'] })]);
    http.expectOne(`${API_BASE}/services/s/recovery-strategies`).flush([]);
    http
      .expectOne(`${API_BASE}/services/s/workflow`)
      .flush({ serviceId: 's', completionPercent: 0, steps: [] });
    if (queryRunbook) {
      tick(fixture);
      return fixture;
    }
    await settle(fixture);
    return fixture;
  }

  it('shows the runbook and a create action for the component without one', async () => {
    const http = setup();
    const fixture = await render(http);
    const rows = [...fixture.nativeElement.querySelectorAll('.row-item')] as HTMLElement[];
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain('Runbook r1');
    expect(rows[1].classList).toContain('missing');
    expect(rows[1].textContent).toContain('Create runbook');
  });

  it('creates a runbook for the scenario and component of the missing row', async () => {
    const http = setup();
    const fixture = await render(http);
    const missing = fixture.nativeElement.querySelector('.row-item.missing') as HTMLElement;
    (missing.querySelector('button') as HTMLButtonElement).click();
    await settle(fixture);
    const dialog = document.querySelector('app-edit-dialog') as HTMLElement;
    const title = dialog.querySelector('input') as HTMLInputElement;
    title.value = 'Rebuild app';
    title.dispatchEvent(new Event('input'));
    await settle(fixture);
    (dialog.querySelector('button[type="submit"]') as HTMLButtonElement).click();
    const req = await vi.waitFor(() => {
      tick(fixture);
      return http.expectOne(`${API_BASE}/microservices/app/runbooks`);
    });
    expect(req.request.body).toMatchObject({ scenarioId: 'ransomware', title: 'Rebuild app' });
  });

  it('opens the context menu from the keyboard (context-menu key)', async () => {
    const http = setup();
    const fixture = await render(http);
    const row = fixture.nativeElement.querySelector('.row-item') as HTMLElement;
    row.focus();
    // The context-menu key fires a contextmenu event without pointer coordinates.
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
    await settle(fixture);
    expect(menuLabels()).toEqual(
      expect.arrayContaining(['edit_noteEdit steps', 'editEdit runbook', 'deleteDelete']),
    );
  });

  it('reopens the step editor from the query parameter', async () => {
    const http = setup({ runbook: 'r1' });
    const fixture = await render(http, true);
    expect(fixture.nativeElement.querySelector('app-runbook-editor')).not.toBeNull();
    http.expectOne(`${API_BASE}/runbooks/r1`);
  });
});

describe('RunbookEditor', () => {
  afterEach(() => document.querySelector('.cdk-overlay-container')?.replaceChildren());

  const detail: RunbookDetail = {
    ...runbook('rb', 's1', 'db'),
    stepCount: 3,
    criticalPathMinutes: 30,
    steps: [
      step('a', 1, 'activation'),
      step('b', 2, 'recovery', { ownerRoleId: 'dba' }),
      step('c', 3, 'reconstitution', { dependsOnStepIds: ['b'] }),
    ],
  };

  async function render(http: HttpTestingController, issues: Issue[] = []) {
    const fixture = TestBed.createComponent(RunbookEditor);
    fixture.componentRef.setInput('runbookId', 'rb');
    fixture.componentRef.setInput('issues', issues);
    tick(fixture);
    http.expectOne(`${API_BASE}/runbooks/rb`).flush(detail);
    http
      .expectOne(`${API_BASE}/roles`)
      .flush([{ ...meta, id: 'dba', name: 'DBA', isDefault: true }]);
    http.expectOne(`${API_BASE}/microservices/db/recovery-objectives`).flush([]);
    await settle(fixture);
    return fixture;
  }

  async function menuAction(fixture: ComponentFixture<unknown>, stepIndex: number, label: string) {
    const steps = fixture.nativeElement.querySelectorAll('.step') as NodeListOf<HTMLElement>;
    steps[stepIndex].dispatchEvent(
      new MouseEvent('contextmenu', { bubbles: true, cancelable: true }),
    );
    await settle(fixture);
    const item = [...document.querySelectorAll<HTMLElement>('.mat-mdc-menu-item')].find((e) =>
      e.textContent!.includes(label),
    );
    item!.click();
    tick(fixture);
  }

  it('groups steps by phase with separators', async () => {
    const http = setup();
    const fixture = await render(http);
    const phases = [...fixture.nativeElement.querySelectorAll('.phase')].map((e) =>
      (e as HTMLElement).textContent!.trim(),
    );
    expect(phases).toEqual(['Activation', 'Recovery', 'Reconstitution']);
    expect(fixture.nativeElement.textContent).toContain('DBA');
  });

  it('inserts above a step at its position, offering only earlier steps as dependencies', async () => {
    const http = setup();
    const fixture = await render(http);
    await menuAction(fixture, 2, 'Insert step above');
    await settle(fixture);
    const dialog = document.querySelector('app-edit-dialog') as HTMLElement;
    expect(dialog).not.toBeNull();
    // Fields: phase (select), title (first input), …
    const title = dialog.querySelector('input') as HTMLInputElement;
    title.value = 'Check replication';
    title.dispatchEvent(new Event('input'));
    // Selects: phase, owner, dependencies, approval role. Dependencies offer steps 1 and 2 only.
    const depSelect = dialog.querySelectorAll('mat-select')[2] as HTMLElement;
    depSelect.click();
    await settle(fixture);
    const options = [...document.querySelectorAll('mat-option')].map((o) => o.textContent!.trim());
    expect(options).toEqual(['1. Step a', '2. Step b']);
    (document.querySelector('.cdk-overlay-backdrop') as HTMLElement | null)?.click();
    await settle(fixture);
    (dialog.querySelector('button[type="submit"]') as HTMLButtonElement).click();
    // The dialog closes asynchronously; the request follows.
    const req = await vi.waitFor(() => {
      tick(fixture);
      return http.expectOne(`${API_BASE}/runbooks/rb/steps`);
    });
    expect(req.request.method).toBe('POST');
    expect(req.request.body).toMatchObject({
      title: 'Check replication',
      phase: 'reconstitution',
      position: 3,
    });
  });

  it('shows a rejected reorder inline and reloads the stored order', async () => {
    const http = setup();
    const fixture = await render(http);
    await menuAction(fixture, 2, 'Move up');
    const req = http.expectOne(`${API_BASE}/runbooks/rb/steps/order`);
    expect(req.request.body).toEqual({ stepIds: ['a', 'c', 'b'] });
    req.flush(
      {
        issues: [
          {
            ruleId: 'STEP_ORDER_VIOLATES_DEPENDENCY',
            severity: 'blocking',
            message: 'step `Step c` must come after the steps it depends on',
          },
        ],
      },
      { status: 422, statusText: 'Unprocessable Entity' },
    );
    tick(fixture);
    http.expectOne(`${API_BASE}/runbooks/rb`).flush(detail);
    http.expectOne(`${API_BASE}/roles`).flush([]);
    http.expectOne(`${API_BASE}/microservices/db/recovery-objectives`).flush([]);
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('[role="alert"]').textContent).toContain(
      'must come after',
    );
    const order = [...fixture.nativeElement.querySelectorAll('.seq')].map(
      (e) => (e as HTMLElement).parentElement!.querySelector('strong')!.textContent,
    );
    expect(order).toEqual(['Step a', 'Step b', 'Step c']);
  });

  it('shows gate issues on the affected step', async () => {
    const http = setup();
    const fixture = await render(http, [
      {
        ruleId: 'STEP_WITHOUT_OWNER',
        severity: 'blocking',
        message: 'step `Step c` has no owner role',
        entityType: 'runbook_step',
        entityId: 'c',
      },
    ]);
    const steps = fixture.nativeElement.querySelectorAll('.step') as NodeListOf<HTMLElement>;
    expect(steps[0].querySelector('.issue')).toBeNull();
    expect(steps[2].querySelector('.issue')!.getAttribute('aria-label')).toContain('no owner role');
  });
});
