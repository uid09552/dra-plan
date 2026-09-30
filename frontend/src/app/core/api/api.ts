import { HttpClient, HttpContext, HttpErrorResponse, HttpParams } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable, catchError, of, throwError } from 'rxjs';

import { EXPECT_NOT_FOUND } from '../http/interceptors';
import {
  Bia,
  Catalog,
  CommunicationRule,
  ComplianceItem,
  CustomCategory,
  Dependency,
  DependencyGraph,
  ItService,
  Microservice,
  Page,
  Person,
  ResourceMeta,
  PlanVersion,
  RecoveryRun,
  RecoveryObjective,
  RecoveryStrategy,
  Role,
  RoleAssignment,
  Scenario,
  ScenarioDecision,
  ScenarioSuggestion,
  ServiceReadiness,
  Tenant,
  TenantReadiness,
  TenantSettings,
  WorkflowState,
  WorkflowStep,
} from './models';

export const API_BASE = '/api/v1';

/** Typed client for the REST API. Tenant and language are added by interceptors/backend. */
@Injectable({ providedIn: 'root' })
export class Api {
  private readonly http = inject(HttpClient);

  // Tenant
  tenant(): Observable<Tenant> {
    return this.http.get<Tenant>(`${API_BASE}/tenant`);
  }

  tenants(): Observable<Page<Tenant>> {
    return this.http.get<Page<Tenant>>(`${API_BASE}/tenants`);
  }

  updateTenantSettings(settings: Partial<TenantSettings>): Observable<Tenant> {
    return this.http.patch<Tenant>(`${API_BASE}/tenant`, { settings }, mergePatch);
  }

  catalog(): Observable<Catalog> {
    return this.http.get<Catalog>(`${API_BASE}/catalog`);
  }

  categories(): Observable<CustomCategory[]> {
    return this.http.get<CustomCategory[]>(`${API_BASE}/categories`);
  }

  createCategory(body: { key: string; label: string }): Observable<CustomCategory> {
    return this.http.post<CustomCategory>(`${API_BASE}/categories`, body);
  }

  deleteCategory(id: string): Observable<void> {
    return this.http.delete<void>(`${API_BASE}/categories/${id}`);
  }

  // Directory
  persons(): Observable<Page<Person>> {
    return this.http.get<Page<Person>>(`${API_BASE}/persons`, {
      params: new HttpParams().set('limit', 200),
    });
  }

  createPerson(body: Partial<Person>): Observable<Person> {
    return this.http.post<Person>(`${API_BASE}/persons`, body);
  }

  roles(): Observable<Role[]> {
    return this.http.get<Role[]>(`${API_BASE}/roles`);
  }

  // Roles & communication of a service (workflow steps 10–11)
  roleAssignments(serviceId: string): Observable<RoleAssignment[]> {
    return this.http.get<RoleAssignment[]>(`${API_BASE}/services/${serviceId}/role-assignments`);
  }

  createRoleAssignment(
    serviceId: string,
    body: Partial<RoleAssignment>,
  ): Observable<RoleAssignment> {
    return this.http.post<RoleAssignment>(
      `${API_BASE}/services/${serviceId}/role-assignments`,
      body,
    );
  }

  updateRoleAssignment(id: string, body: Partial<RoleAssignment>): Observable<RoleAssignment> {
    return this.http.patch<RoleAssignment>(`${API_BASE}/role-assignments/${id}`, body, mergePatch);
  }

  deleteRoleAssignment(id: string): Observable<void> {
    return this.http.delete<void>(`${API_BASE}/role-assignments/${id}`);
  }

  communicationRules(serviceId: string): Observable<CommunicationRule[]> {
    return this.http.get<CommunicationRule[]>(
      `${API_BASE}/services/${serviceId}/communication-rules`,
    );
  }

  createCommunicationRule(
    serviceId: string,
    body: Partial<CommunicationRule>,
  ): Observable<CommunicationRule> {
    return this.http.post<CommunicationRule>(
      `${API_BASE}/services/${serviceId}/communication-rules`,
      body,
    );
  }

  /** Merge patch: `null` clears optional fields. */
  updateCommunicationRule(
    id: string,
    body: Omit<Partial<CommunicationRule>, 'authorizerRoleId' | 'frequencyMinutes'> & {
      authorizerRoleId?: string | null;
      frequencyMinutes?: number | null;
    },
  ): Observable<CommunicationRule> {
    return this.http.patch<CommunicationRule>(
      `${API_BASE}/communication-rules/${id}`,
      body,
      mergePatch,
    );
  }

  deleteCommunicationRule(id: string): Observable<void> {
    return this.http.delete<void>(`${API_BASE}/communication-rules/${id}`);
  }

  // IT services
  services(query?: string): Observable<Page<ItService>> {
    let params = new HttpParams().set('limit', 200);
    if (query) {
      params = params.set('q', query);
    }
    return this.http.get<Page<ItService>>(`${API_BASE}/services`, { params });
  }

  service(id: string): Observable<ItService> {
    return this.http.get<ItService>(`${API_BASE}/services/${id}`);
  }

  createService(body: Partial<ItService>): Observable<ItService> {
    return this.http.post<ItService>(`${API_BASE}/services`, body);
  }

  updateService(
    id: string,
    body: Partial<Omit<ItService, 'businessOwnerId' | 'technicalOwnerId'>> & {
      businessOwnerId?: string | null;
      technicalOwnerId?: string | null;
    },
  ): Observable<ItService> {
    return this.http.patch<ItService>(`${API_BASE}/services/${id}`, body, mergePatch);
  }

  // Readiness & compliance
  readiness(): Observable<TenantReadiness> {
    return this.http.get<TenantReadiness>(`${API_BASE}/readiness`);
  }

  serviceReadiness(serviceId: string): Observable<ServiceReadiness> {
    return this.http.get<ServiceReadiness>(`${API_BASE}/services/${serviceId}/readiness`);
  }

  compliance(serviceId: string): Observable<ComplianceItem[]> {
    return this.http.get<ComplianceItem[]>(`${API_BASE}/services/${serviceId}/compliance`);
  }

  // Business impact
  bia(serviceId: string): Observable<Bia | null> {
    return this.http
      .get<Bia>(`${API_BASE}/services/${serviceId}/bia`, {
        context: new HttpContext().set(EXPECT_NOT_FOUND, true),
      })
      .pipe(catchError((e: unknown) => (isNotFound(e) ? of(null) : throwError(() => e))));
  }

  putBia(serviceId: string, body: Omit<Bia, keyof ResourceMeta | 'serviceId'>): Observable<Bia> {
    return this.http.put<Bia>(`${API_BASE}/services/${serviceId}/bia`, body);
  }

  // Dependencies
  dependencies(microserviceId: string): Observable<Dependency[]> {
    return this.http.get<Dependency[]>(`${API_BASE}/microservices/${microserviceId}/dependencies`);
  }

  createDependency(microserviceId: string, body: Partial<Dependency>): Observable<Dependency> {
    return this.http.post<Dependency>(
      `${API_BASE}/microservices/${microserviceId}/dependencies`,
      body,
    );
  }

  updateDependency(id: string, body: Partial<Dependency>): Observable<Dependency> {
    return this.http.patch<Dependency>(`${API_BASE}/dependencies/${id}`, body, mergePatch);
  }

  deleteDependency(id: string): Observable<void> {
    return this.http.delete<void>(`${API_BASE}/dependencies/${id}`);
  }

  dependencyGraph(serviceId: string): Observable<DependencyGraph> {
    return this.http.get<DependencyGraph>(`${API_BASE}/services/${serviceId}/dependency-graph`);
  }

  // Objectives & strategies
  objectives(microserviceId: string): Observable<RecoveryObjective[]> {
    return this.http.get<RecoveryObjective[]>(
      `${API_BASE}/microservices/${microserviceId}/recovery-objectives`,
    );
  }

  createObjective(
    microserviceId: string,
    body: { rtoMinutes: number; rpoMinutes: number; scenarioId?: string },
  ): Observable<RecoveryObjective> {
    return this.http.post<RecoveryObjective>(
      `${API_BASE}/microservices/${microserviceId}/recovery-objectives`,
      body,
    );
  }

  updateObjective(
    id: string,
    body: { rtoMinutes?: number; rpoMinutes?: number },
  ): Observable<RecoveryObjective> {
    return this.http.patch<RecoveryObjective>(
      `${API_BASE}/recovery-objectives/${id}`,
      body,
      mergePatch,
    );
  }

  deleteObjective(id: string): Observable<void> {
    return this.http.delete<void>(`${API_BASE}/recovery-objectives/${id}`);
  }

  /** All measures of a service, across its components. */
  serviceStrategies(serviceId: string): Observable<RecoveryStrategy[]> {
    return this.http.get<RecoveryStrategy[]>(
      `${API_BASE}/services/${serviceId}/recovery-strategies`,
    );
  }

  strategies(microserviceId: string): Observable<RecoveryStrategy[]> {
    return this.http.get<RecoveryStrategy[]>(
      `${API_BASE}/microservices/${microserviceId}/recovery-strategies`,
    );
  }

  createStrategy(
    microserviceId: string,
    body: Partial<RecoveryStrategy> & { scenarioIds: string[]; type: string },
  ): Observable<RecoveryStrategy> {
    return this.http.post<RecoveryStrategy>(
      `${API_BASE}/microservices/${microserviceId}/recovery-strategies`,
      body,
    );
  }

  updateStrategy(
    id: string,
    body: Partial<Omit<RecoveryStrategy, 'lastTestedAt'>> & { lastTestedAt?: string | null },
  ): Observable<RecoveryStrategy> {
    return this.http.patch<RecoveryStrategy>(
      `${API_BASE}/recovery-strategies/${id}`,
      body,
      mergePatch,
    );
  }

  deleteStrategy(id: string): Observable<void> {
    return this.http.delete<void>(`${API_BASE}/recovery-strategies/${id}`);
  }

  selectStrategy(
    id: string,
    body: { acceptGap?: boolean; gapJustification?: string } = {},
  ): Observable<RecoveryStrategy> {
    return this.http.post<RecoveryStrategy>(`${API_BASE}/recovery-strategies/${id}/select`, body);
  }

  // Workflow
  workflow(serviceId: string): Observable<WorkflowState> {
    return this.http.get<WorkflowState>(`${API_BASE}/services/${serviceId}/workflow`);
  }

  completeStep(
    serviceId: string,
    key: string,
    acknowledgeWarnings = false,
  ): Observable<WorkflowStep> {
    return this.http.post<WorkflowStep>(
      `${API_BASE}/services/${serviceId}/workflow/steps/${key}/complete`,
      {
        acknowledgeWarnings,
      },
    );
  }

  reopenStep(serviceId: string, key: string): Observable<WorkflowState> {
    return this.http.post<WorkflowState>(
      `${API_BASE}/services/${serviceId}/workflow/steps/${key}/reopen`,
      {},
    );
  }

  // Microservices & scenarios
  microservices(serviceId: string): Observable<Microservice[]> {
    return this.http.get<Microservice[]>(`${API_BASE}/services/${serviceId}/microservices`);
  }

  createMicroservice(serviceId: string, body: Partial<Microservice>): Observable<Microservice> {
    return this.http.post<Microservice>(`${API_BASE}/services/${serviceId}/microservices`, body);
  }

  updateMicroservice(id: string, body: Partial<Microservice>): Observable<Microservice> {
    return this.http.patch<Microservice>(`${API_BASE}/microservices/${id}`, body, mergePatch);
  }

  deleteMicroservice(id: string): Observable<void> {
    return this.http.delete<void>(`${API_BASE}/microservices/${id}`);
  }

  deleteScenario(id: string): Observable<void> {
    return this.http.delete<void>(`${API_BASE}/scenarios/${id}`);
  }

  scenarios(serviceId: string): Observable<Scenario[]> {
    return this.http.get<Scenario[]>(`${API_BASE}/services/${serviceId}/scenarios`);
  }

  createScenario(
    serviceId: string,
    body: {
      title?: string;
      description?: string;
      category?: string;
      catalogTemplateId?: string;
      parentScenarioId?: string;
      affectedMicroserviceIds?: string[];
    },
  ): Observable<Scenario> {
    return this.http.post<Scenario>(`${API_BASE}/services/${serviceId}/scenarios`, body);
  }

  /** Merge patch: `null` clears a field. */
  updateScenario(
    id: string,
    body: {
      title?: string;
      description?: string;
      category?: string | null;
      parentScenarioId?: string | null;
      likelihood?: number;
      impact?: number;
      affectedMicroserviceIds?: string[];
    },
  ): Observable<Scenario> {
    return this.http.patch<Scenario>(`${API_BASE}/scenarios/${id}`, body, mergePatch);
  }

  /** Merges the source scenarios into the target (the sources get status `merged`). */
  mergeScenarios(targetId: string, sourceScenarioIds: string[]): Observable<Scenario> {
    return this.http.post<Scenario>(`${API_BASE}/scenarios/${targetId}/merge`, {
      sourceScenarioIds,
    });
  }

  decideScenario(id: string, body: ScenarioDecision): Observable<Scenario> {
    return this.http.post<Scenario>(`${API_BASE}/scenarios/${id}/decision`, body);
  }

  scenarioSuggestions(serviceId: string): Observable<ScenarioSuggestion[]> {
    return this.http.get<ScenarioSuggestion[]>(
      `${API_BASE}/services/${serviceId}/scenario-suggestions`,
    );
  }

  // Plans
  planVersions(serviceId: string): Observable<PlanVersion[]> {
    return this.http.get<PlanVersion[]>(`${API_BASE}/services/${serviceId}/plan-versions`);
  }

  submitPlan(serviceId: string): Observable<PlanVersion> {
    return this.http.post<PlanVersion>(`${API_BASE}/services/${serviceId}/plan-versions`, {});
  }

  approvePlan(id: string): Observable<PlanVersion> {
    return this.http.post<PlanVersion>(`${API_BASE}/plan-versions/${id}/approve`, {});
  }

  exportUrl(id: string): string {
    return `${API_BASE}/plan-versions/${id}/export?format=markdown`;
  }

  exportMarkdown(id: string): Observable<string> {
    return this.http.get(this.exportUrl(id), { responseType: 'text' });
  }

  /** Live draft of the emergency handbook (Markdown, not approved). */
  draftHandbook(serviceId: string): Observable<string> {
    return this.http.get(`${API_BASE}/services/${serviceId}/handbook`, { responseType: 'text' });
  }

  // Recovery
  recoveryRun(id: string): Observable<RecoveryRun> {
    return this.http.get<RecoveryRun>(`${API_BASE}/recovery-runs/${id}`);
  }

  recoveryRuns(serviceId: string): Observable<RecoveryRun[]> {
    return this.http.get<RecoveryRun[]>(`${API_BASE}/services/${serviceId}/recovery-runs`);
  }
}

const mergePatch = { headers: { 'Content-Type': 'application/merge-patch+json' } };

function isNotFound(e: unknown): boolean {
  return e instanceof HttpErrorResponse && e.status === 404;
}
