//! Dependencies of microservices (workflow step 2, NIST resource requirements, BSI Ressourcen)
//! and the dependency graph with RTO conflicts and a suggested restore order.

use std::collections::{BTreeMap, HashMap, HashSet};

use async_trait::async_trait;
use uuid::Uuid;

use crate::features::microservices::domain::Microservice;
use crate::features::objectives::domain::{RecoveryObjective, effective};
use crate::shared::kernel::{
    AppResult, Issues, Meta, Minutes, Provenance, TenantContext, non_blank, str_enum,
};

str_enum! {
    pub enum DependencyKind {
        Microservice = "microservice",
        Infrastructure = "infrastructure",
        Platform = "platform",
        ExternalService = "external_service",
        Supplier = "supplier",
        Personnel = "personnel",
    }
}

str_enum! {
    /// `upstream`: this microservice depends on the target. `downstream`: the target depends on it.
    pub enum DependencyDirection { Upstream = "upstream", Downstream = "downstream" }
}

str_enum! {
    pub enum DependencyCriticality { Critical = "critical", Degradable = "degradable", Optional = "optional" }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub meta: Meta,
    pub provenance: Provenance,
    pub microservice_id: Uuid,
    pub kind: DependencyKind,
    pub target_microservice_id: Option<Uuid>,
    pub target_name: Option<String>,
    pub direction: DependencyDirection,
    pub criticality: DependencyCriticality,
    pub dependency_rto: Option<Minutes>,
    pub has_own_dr_plan: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DependencyInput {
    pub kind: Option<DependencyKind>,
    pub target_microservice_id: Option<Option<Uuid>>,
    pub target_name: Option<String>,
    pub direction: Option<DependencyDirection>,
    pub criticality: Option<DependencyCriticality>,
    pub dependency_rto: Option<Option<Minutes>>,
    pub has_own_dr_plan: Option<bool>,
    pub notes: Option<String>,
}

impl Dependency {
    pub fn create(
        ctx: &TenantContext,
        microservice_id: Uuid,
        kind: DependencyKind,
        direction: DependencyDirection,
        criticality: DependencyCriticality,
        input: DependencyInput,
    ) -> AppResult<Self> {
        let mut d = Dependency {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            microservice_id,
            kind,
            target_microservice_id: None,
            target_name: None,
            direction,
            criticality,
            dependency_rto: None,
            has_own_dr_plan: false,
            notes: None,
        };
        d.apply(input)?;
        Ok(d)
    }

    pub fn apply(&mut self, p: DependencyInput) -> AppResult<()> {
        if let Some(v) = p.kind {
            self.kind = v;
        }
        if let Some(v) = p.target_microservice_id {
            self.target_microservice_id = v;
        }
        if p.target_name.is_some() {
            self.target_name = non_blank(p.target_name);
        }
        if let Some(v) = p.direction {
            self.direction = v;
        }
        if let Some(v) = p.criticality {
            self.criticality = v;
        }
        if let Some(v) = p.dependency_rto {
            self.dependency_rto = v;
        }
        if let Some(v) = p.has_own_dr_plan {
            self.has_own_dr_plan = v;
        }
        if p.notes.is_some() {
            self.notes = non_blank(p.notes);
        }
        if self.kind != DependencyKind::Microservice {
            self.target_microservice_id = None;
        }
        let mut issues = Issues::new();
        issues.check(
            self.kind != DependencyKind::Microservice || self.target_microservice_id.is_some(),
            "REQUIRED",
            "/targetMicroserviceId",
            "targetMicroserviceId is required for microservice dependencies",
        );
        issues.check(
            self.kind == DependencyKind::Microservice || self.target_name.is_some(),
            "REQUIRED",
            "/targetName",
            "targetName is required for non-microservice dependencies",
        );
        issues.check(
            self.target_microservice_id != Some(self.microservice_id),
            "INVALID_VALUE",
            "/targetMicroserviceId",
            "a microservice cannot depend on itself",
        );
        issues.into_result()
    }

    /// Recovery time of the dependency: explicit, else the target microservice's default objective.
    pub fn effective_rto(&self, objectives: &[RecoveryObjective]) -> Option<Minutes> {
        self.dependency_rto.or_else(|| {
            self.target_microservice_id
                .and_then(|t| effective(objectives, t, None))
                .map(|o| o.rto)
        })
    }

    /// Integrity rule 3: a critical upstream dependency must recover at least as fast as its dependent.
    pub fn rto_conflict(&self, objectives: &[RecoveryObjective]) -> bool {
        if self.criticality != DependencyCriticality::Critical
            || self.direction != DependencyDirection::Upstream
        {
            return false;
        }
        let own = effective(objectives, self.microservice_id, None).map(|o| o.rto);
        matches!((own, self.effective_rto(objectives)), (Some(own), Some(dep)) if dep > own)
    }
}

#[async_trait]
pub trait DependencyRepository: Send + Sync {
    async fn list_by_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
    ) -> AppResult<Vec<Dependency>>;
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<Dependency>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Dependency>>;
    async fn insert(&self, ctx: &TenantContext, d: &Dependency) -> AppResult<Dependency>;
    async fn update(&self, ctx: &TenantContext, d: &Dependency) -> AppResult<Dependency>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()>;
}

// ───────────────────────────── Graph ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub node_type: String,
    pub label: String,
    pub rto: Option<Minutes>,
    /// Set when the node is a single point of failure (see [`SpofReason`]).
    pub single_point_of_failure: Option<SpofReason>,
}

/// Why a dependency is a single point of failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpofReason {
    /// Two or more microservices of the service depend on it critically.
    SharedCriticalDependency,
    /// Critical dependency outside the service without its own DR plan.
    CriticalWithoutDrPlan,
}

impl SpofReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SpofReason::SharedCriticalDependency => "shared_critical_dependency",
            SpofReason::CriticalWithoutDrPlan => "critical_without_dr_plan",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    /// Dependent node.
    pub from: String,
    /// Node it depends on.
    pub to: String,
    pub dependency_id: Uuid,
    pub criticality: DependencyCriticality,
    pub rto_conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DependencyGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub suggested_restore_order: Vec<Uuid>,
    pub cycles: Vec<Vec<String>>,
}

/// Builds the graph of one IT service. `external` holds microservices of other services that are
/// dependency targets.
/// Components shown in the graph: the default component (whole service) is left out when explicit
/// components exist and no dependency involves it, so it does not clutter map and restore order.
pub fn graph_components(
    microservices: Vec<Microservice>,
    deps: &[Dependency],
) -> Vec<Microservice> {
    let explicit = microservices.iter().any(|m| !m.is_default);
    let involved = |id: Uuid| {
        deps.iter()
            .any(|d| d.microservice_id == id || d.target_microservice_id == Some(id))
    };
    microservices
        .into_iter()
        .filter(|m| !(explicit && m.is_default && !involved(m.meta.id)))
        .collect()
}

pub fn build_graph(
    microservices: &[Microservice],
    external: &[Microservice],
    dependencies: &[Dependency],
    objectives: &[RecoveryObjective],
) -> DependencyGraph {
    let own: HashSet<Uuid> = microservices.iter().map(|m| m.meta.id).collect();
    let mut nodes: BTreeMap<String, GraphNode> = BTreeMap::new();
    for m in microservices {
        nodes.insert(
            m.meta.id.to_string(),
            GraphNode {
                id: m.meta.id.to_string(),
                node_type: "microservice".into(),
                label: m.name.clone(),
                rto: effective(objectives, m.meta.id, None).map(|o| o.rto),
                single_point_of_failure: None,
            },
        );
    }
    let external_names: HashMap<Uuid, &str> = external
        .iter()
        .map(|m| (m.meta.id, m.name.as_str()))
        .collect();

    let mut edges = Vec::new();
    for d in dependencies {
        let target_id = match d.target_microservice_id {
            Some(t) => {
                let id = t.to_string();
                if !own.contains(&t) {
                    nodes.entry(id.clone()).or_insert_with(|| GraphNode {
                        id: id.clone(),
                        node_type: "external_microservice".into(),
                        label: external_names
                            .get(&t)
                            .map_or_else(|| id.clone(), |n| (*n).to_owned()),
                        rto: d.effective_rto(objectives),
                        single_point_of_failure: None,
                    });
                }
                id
            }
            None => {
                let name = d.target_name.clone().unwrap_or_default();
                let id = format!("{}:{}", d.kind, name.to_lowercase());
                nodes.entry(id.clone()).or_insert_with(|| GraphNode {
                    id: id.clone(),
                    node_type: d.kind.as_str().into(),
                    label: name,
                    rto: d.dependency_rto,
                    single_point_of_failure: None,
                });
                id
            }
        };
        let own_id = d.microservice_id.to_string();
        let (from, to) = match d.direction {
            DependencyDirection::Upstream => (own_id, target_id),
            DependencyDirection::Downstream => (target_id, own_id),
        };
        edges.push(GraphEdge {
            from,
            to,
            dependency_id: d.meta.id,
            criticality: d.criticality,
            rto_conflict: d.rto_conflict(objectives),
        });
    }

    mark_single_points_of_failure(&mut nodes, &edges, dependencies, &own);
    let (suggested_restore_order, cycles) = restore_order(microservices, &edges);
    DependencyGraph {
        nodes: nodes.into_values().collect(),
        edges,
        suggested_restore_order,
        cycles,
    }
}

/// Flags nodes that critical parts of the service cannot recover without.
fn mark_single_points_of_failure(
    nodes: &mut BTreeMap<String, GraphNode>,
    edges: &[GraphEdge],
    dependencies: &[Dependency],
    own: &HashSet<Uuid>,
) {
    let mut critical_dependents: HashMap<&str, HashSet<&str>> = HashMap::new();
    for e in edges
        .iter()
        .filter(|e| e.criticality == DependencyCriticality::Critical)
    {
        critical_dependents
            .entry(e.to.as_str())
            .or_default()
            .insert(e.from.as_str());
    }
    let without_plan: HashSet<String> = dependencies
        .iter()
        .filter(|d| d.criticality == DependencyCriticality::Critical && !d.has_own_dr_plan)
        .filter(|d| d.target_microservice_id.is_none_or(|t| !own.contains(&t)))
        .filter_map(|d| {
            edges
                .iter()
                .find(|e| e.dependency_id == d.meta.id)
                .map(|e| e.to.clone())
        })
        .collect();
    for (id, node) in nodes.iter_mut() {
        let shared = critical_dependents
            .get(id.as_str())
            .is_some_and(|from| from.len() >= 2);
        node.single_point_of_failure = if shared {
            Some(SpofReason::SharedCriticalDependency)
        } else if without_plan.contains(id) {
            Some(SpofReason::CriticalWithoutDrPlan)
        } else {
            None
        };
    }
}

/// Topological order of the service's microservices (dependencies first, Kahn's algorithm).
/// Ties are broken by the configured restore order, then by name. Nodes on cycles are appended
/// at the end and reported.
fn restore_order(
    microservices: &[Microservice],
    edges: &[GraphEdge],
) -> (Vec<Uuid>, Vec<Vec<String>>) {
    let ids: HashMap<String, &Microservice> = microservices
        .iter()
        .map(|m| (m.meta.id.to_string(), m))
        .collect();
    // prerequisites[x] = nodes x depends on (within the service)
    let mut remaining: HashMap<&str, HashSet<&str>> =
        ids.keys().map(|k| (k.as_str(), HashSet::new())).collect();
    for e in edges {
        if ids.contains_key(&e.from) && ids.contains_key(&e.to) && e.from != e.to {
            if let Some(set) = remaining.get_mut(e.from.as_str()) {
                set.insert(e.to.as_str());
            }
        }
    }
    let sort_key = |id: &str| {
        let m = ids[id];
        (
            m.restore_order.unwrap_or(u32::MAX),
            m.name.clone(),
            id.to_owned(),
        )
    };
    let mut order = Vec::new();
    loop {
        let mut ready: Vec<&str> = remaining
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(id, _)| *id)
            .collect();
        if ready.is_empty() {
            break;
        }
        ready.sort_by_key(|id| sort_key(id));
        for id in &ready {
            remaining.remove(id);
        }
        for deps in remaining.values_mut() {
            for id in &ready {
                deps.remove(id);
            }
        }
        order.extend(ready.into_iter().map(|id| ids[id].meta.id));
    }
    let mut cycles = Vec::new();
    if !remaining.is_empty() {
        let mut stuck: Vec<&str> = remaining.keys().copied().collect();
        stuck.sort_by_key(|id| sort_key(id));
        cycles.push(stuck.iter().map(|s| (*s).to_owned()).collect());
        order.extend(stuck.into_iter().map(|id| ids[id].meta.id));
    }
    (order, cycles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::microservices::domain::MicroserviceInput;
    use crate::features::objectives::domain::ObjectiveInput;
    use crate::shared::kernel::{Principal, TenantId, TenantRole};

    fn ctx() -> TenantContext {
        TenantContext {
            tenant_id: TenantId(Uuid::nil()),
            principal: Principal {
                user: "u".into(),
                role: TenantRole::Admin,
            },
        }
    }

    fn ms(name: &str) -> Microservice {
        Microservice::create(
            &ctx(),
            Uuid::nil(),
            MicroserviceInput {
                name: Some(name.into()),
                ..Default::default()
            },
        )
        .expect("valid")
    }

    fn dep(
        from: &Microservice,
        to: &Microservice,
        criticality: DependencyCriticality,
    ) -> Dependency {
        Dependency::create(
            &ctx(),
            from.meta.id,
            DependencyKind::Microservice,
            DependencyDirection::Upstream,
            criticality,
            DependencyInput {
                target_microservice_id: Some(Some(to.meta.id)),
                ..Default::default()
            },
        )
        .expect("valid")
    }

    fn objective(m: &Microservice, rto: u32) -> RecoveryObjective {
        let min = |v| Minutes::new(v).expect("valid");
        RecoveryObjective::create(
            &ctx(),
            m.meta.id,
            min(rto),
            min(5),
            ObjectiveInput::default(),
        )
        .expect("valid")
    }

    #[test]
    fn orders_dependencies_first_and_flags_rto_conflicts() {
        let (api, db, cache) = (ms("api"), ms("db"), ms("cache"));
        let deps = vec![
            dep(&api, &db, DependencyCriticality::Critical),
            dep(&api, &cache, DependencyCriticality::Optional),
        ];
        let objectives = vec![objective(&api, 60), objective(&db, 120)];
        let graph = build_graph(
            &[api.clone(), db.clone(), cache.clone()],
            &[],
            &deps,
            &objectives,
        );
        let pos = |m: &Microservice| {
            graph
                .suggested_restore_order
                .iter()
                .position(|id| *id == m.meta.id)
        };
        assert!(pos(&db) < pos(&api));
        assert!(pos(&cache) < pos(&api));
        assert!(
            graph
                .edges
                .iter()
                .any(|e| e.rto_conflict && e.to == db.meta.id.to_string())
        );
        assert!(graph.cycles.is_empty());
    }

    #[test]
    fn detects_single_points_of_failure() {
        let (api, worker, db) = (ms("api"), ms("worker"), ms("db"));
        let mut deps = vec![
            dep(&api, &db, DependencyCriticality::Critical),
            dep(&worker, &db, DependencyCriticality::Critical),
        ];
        deps.push(
            Dependency::create(
                &ctx(),
                api.meta.id,
                DependencyKind::ExternalService,
                DependencyDirection::Upstream,
                DependencyCriticality::Critical,
                DependencyInput {
                    target_name: Some("Payment provider".into()),
                    ..Default::default()
                },
            )
            .expect("valid"),
        );
        let graph = build_graph(&[api, worker, db.clone()], &[], &deps, &[]);
        let spof = |id: &str| {
            graph
                .nodes
                .iter()
                .find(|n| n.id == id)
                .and_then(|n| n.single_point_of_failure)
        };
        assert_eq!(
            spof(&db.meta.id.to_string()),
            Some(SpofReason::SharedCriticalDependency)
        );
        assert_eq!(
            spof("external_service:payment provider"),
            Some(SpofReason::CriticalWithoutDrPlan)
        );
    }

    #[test]
    fn reports_cycles() {
        let (a, b) = (ms("a"), ms("b"));
        let deps = vec![
            dep(&a, &b, DependencyCriticality::Critical),
            dep(&b, &a, DependencyCriticality::Critical),
        ];
        let graph = build_graph(&[a, b], &[], &deps, &[]);
        assert_eq!(graph.cycles.len(), 1);
        assert_eq!(graph.suggested_restore_order.len(), 2);
    }

    #[test]
    fn rejects_self_dependency() {
        let a = ms("a");
        let result = Dependency::create(
            &ctx(),
            a.meta.id,
            DependencyKind::Microservice,
            DependencyDirection::Upstream,
            DependencyCriticality::Critical,
            DependencyInput {
                target_microservice_id: Some(Some(a.meta.id)),
                ..Default::default()
            },
        );
        assert!(result.is_err());
    }
}
