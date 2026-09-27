/** Types of the REST API (api/openapi.yaml). Only the fields the UI uses are declared. */

export interface Page<T> {
  items: T[];
  nextCursor?: string | null;
}

export interface ResourceMeta {
  id: string;
  tenantId: string;
  version: number;
  createdAt: string;
  createdBy: string;
  updatedAt: string;
  updatedBy: string;
}

export interface Issue {
  ruleId: string;
  severity: 'blocking' | 'warning' | 'info';
  message: string;
  field?: string;
  entityType?: string;
  entityId?: string;
  workflowStep?: string;
  standardRef?: string;
}

export interface Problem {
  title: string;
  status: number;
  detail?: string;
  issues?: Issue[];
}

export interface TenantSettings {
  reviewIntervalMonths: number;
  impactCategories: string[];
  impactTimeWindowsMinutes: number[];
  impactToleranceLevel: number;
  defaultLanguage: 'en' | 'de';
  aiEnabled: boolean;
}

export interface Tenant extends ResourceMeta {
  name: string;
  slug: string;
  settings: TenantSettings;
}

export interface Person extends ResourceMeta {
  name: string;
  email?: string;
  phone?: string;
  alternateContact?: string;
  team?: string;
}

export type PlanStatus = 'draft' | 'in_review' | 'approved' | 'retired';

export interface ServiceSummary {
  microserviceCount: number;
  selectedScenarioCount: number;
  workflowCompletionPercent: number;
  currentPlanVersionId?: string;
  currentPlanStatus?: PlanStatus;
  nextReviewDue?: string;
  lastTestedAt?: string;
  openActionItemCount: number;
  activeRecoveryRunId?: string;
}

export type ProtectionRequirement = 'normal' | 'high' | 'very_high';
export type ImpactLevel = 'low' | 'moderate' | 'high';

export interface ItService extends ResourceMeta {
  name: string;
  description?: string;
  businessOwnerId?: string;
  technicalOwnerId?: string;
  protectionRequirementAvailability?: ProtectionRequirement;
  impactLevel?: ImpactLevel;
  lifecycleStatus: 'planned' | 'active' | 'retired';
  summary?: ServiceSummary;
}

export interface Microservice extends ResourceMeta {
  serviceId: string;
  name: string;
  description?: string;
  platform?: string;
  hostingLocation?: string;
  dataStores: string[];
  restoreOrder?: number;
  /** Stands for the whole service; created with the service, cannot be deleted. */
  isDefault: boolean;
}

export type ScenarioStatus = 'brainstormed' | 'merged' | 'selected' | 'rejected';

export interface Scenario extends ResourceMeta {
  serviceId: string;
  title: string;
  description?: string;
  category?: string;
  parentScenarioId?: string;
  /** Empty = the whole service (default component). */
  affectedMicroserviceIds: string[];
  status: ScenarioStatus;
  likelihood?: number;
  impact?: number;
  riskScore?: number;
  priority?: string;
  drRequired?: string;
  decisionRationale?: string;
}

export interface ScenarioDecision {
  decision: 'selected' | 'rejected';
  decisionRationale: string;
  likelihood?: number;
  impact?: number;
  drRequired?: string;
}

export interface ScenarioSuggestion {
  templateId: string;
  category: string;
  title: string;
  description: string;
  relevance: string;
}

export interface Bia extends ResourceMeta {
  serviceId: string;
  mtpdMinutes: number;
  serviceRtoMinutes: number;
  serviceRpoMinutes: number;
  minimumOperatingLevel?: string;
  regulatoryRequirements?: string;
  impactRatings: {
    impactCategory: string;
    timeWindowMinutes: number;
    level: number;
    rationale?: string;
  }[];
}

export type DependencyKind =
  'microservice' | 'infrastructure' | 'platform' | 'external_service' | 'supplier' | 'personnel';

export interface Dependency extends ResourceMeta {
  microserviceId: string;
  kind: DependencyKind;
  targetMicroserviceId?: string;
  targetName?: string;
  direction: 'upstream' | 'downstream';
  criticality: 'critical' | 'degradable' | 'optional';
  dependencyRtoMinutes?: number;
  hasOwnDrPlan: boolean;
}

export interface GraphNode {
  id: string;
  type: string;
  label: string;
  rtoMinutes?: number | null;
  singlePointOfFailure?: 'shared_critical_dependency' | 'critical_without_dr_plan';
}

export interface DependencyGraph {
  nodes: GraphNode[];
  edges: { from: string; to: string; criticality: string; rtoConflict: boolean }[];
  suggestedRestoreOrder: string[];
  cycles: string[][];
}

export interface RecoveryObjective extends ResourceMeta {
  microserviceId: string;
  scenarioId?: string;
  rtoMinutes: number;
  rpoMinutes: number;
}

export type ImplementationStatus = 'not_implemented' | 'in_progress' | 'implemented';

export interface RecoveryStrategy extends ResourceMeta {
  microserviceId: string;
  /** Scenarios this measure covers (one or more). */
  scenarioIds: string[];
  type: string;
  title?: string;
  estimatedRtoMinutes: number;
  estimatedRpoMinutes: number;
  implementationStatus: ImplementationStatus;
  lastTestedAt?: string;
  isSelected: boolean;
  gapCheck?: { status: 'meets' | 'gap' | 'no_objective'; rtoGapMinutes?: number };
}

export interface NextAction {
  serviceId?: string;
  serviceName?: string;
  step: string;
  stepNumber: number;
  severity: 'blocking' | 'warning' | 'info';
  ruleId: string;
  message: string;
}

export interface ReadinessFigures {
  score: number;
  critical: number;
  attention: number;
  completed: number;
  total: number;
  lastExerciseAt?: string;
  rtoCompliance?: number;
  untestedScenarios: number;
}

export interface ServiceReadiness extends ReadinessFigures {
  serviceId: string;
  name: string;
  reviewDue: boolean;
  nextActions: NextAction[];
}

export interface TenantReadiness extends ReadinessFigures {
  plansRequiringReview: number;
  nextActions: NextAction[];
  services: ServiceReadiness[];
}

export interface ComplianceItem {
  id: string;
  framework: string;
  reference: string;
  title: string;
  whatToDo: string;
  evidence: string;
  steps: string[];
  status: 'fulfilled' | 'in_progress' | 'open';
  blockingIssues: number;
}

export type StepStatus = 'not_started' | 'in_progress' | 'complete' | 'needs_review';

export interface WorkflowStep {
  key: string;
  number: number;
  phase: 'understand' | 'assess' | 'design' | 'document' | 'validate_maintain';
  title: string;
  status: StepStatus;
  gatePassed: boolean;
  issues: Issue[];
  completedBy?: string;
  completedAt?: string;
}

export interface WorkflowState {
  serviceId: string;
  completionPercent: number;
  currentStepKey?: string;
  steps: WorkflowStep[];
}

export interface PlanVersion {
  id: string;
  serviceId?: string;
  version: number;
  status: PlanStatus;
  submittedBy: string;
  submittedAt: string;
  approvedAt?: string;
  nextReviewDue?: string;
  comment?: string;
}

export interface RecoveryRun {
  id: string;
  serviceId?: string;
  scenarioId?: string;
  mode: 'real' | 'test';
  status: 'declared' | 'in_progress' | 'recovered' | 'reconstituting' | 'closed' | 'aborted';
  declaredAt: string;
  clock: {
    elapsedMinutes: number;
    serviceRtoMinutes?: number;
    remainingCriticalPathMinutes: number;
    rtoAtRisk: boolean;
  };
  progress: {
    total: number;
    done: number;
    skipped: number;
    failed: number;
    inProgress: number;
    blocked: number;
  };
}

export interface Catalog {
  scenarioCategories: { key: string; label: string }[];
  scenarioTemplates: { id: string; category: string; title: string; description: string }[];
  strategyTypes: {
    key: string;
    label: string;
    typicalRto: string;
    typicalRpo: string;
    notes: string;
  }[];
  testTypes: { key: string; label: string; depth: number }[];
  defaultRoles: string[];
  workflowSteps: {
    key: string;
    number: number;
    phase: string;
    title: string;
    nistRef: string;
    bsiRef: string;
  }[];
}

export interface Role extends ResourceMeta {
  name: string;
  description?: string;
  isDefault: boolean;
}

export interface RoleAssignment extends ResourceMeta {
  serviceId: string;
  roleId: string;
  personId: string;
  isDeputy: boolean;
  escalationOrder?: number;
}

export type CommunicationTrigger =
  'dr_declared' | 'status_update' | 'recovered' | 'failback' | 'aborted';
export type CommunicationAudience =
  'internal' | 'management' | 'customers' | 'regulator' | 'suppliers';

export interface CommunicationRule extends ResourceMeta {
  serviceId: string;
  trigger: CommunicationTrigger;
  audience: CommunicationAudience;
  channel: string;
  frequencyMinutes?: number;
  responsibleRoleId: string;
  /** Role that authorizes failover / DR declaration. */
  authorizerRoleId?: string;
  template?: string;
}
