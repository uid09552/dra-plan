use std::fmt::Write as _;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{PlanAggregate, PlanRenderer, PlanStatus, PlanVersion, PlanVersionRepository};
use crate::features::runbooks::domain::{RunbookPhase, critical_path};
use crate::shared::infra::{Db, db_enum, write_err};
use crate::shared::kernel::{AppError, AppResult, Language, Minutes, TenantContext, TenantId};

#[derive(FromRow)]
struct PlanRow {
    id: Uuid,
    tenant_id: Uuid,
    service_id: Option<Uuid>,
    plan_number: i32,
    status: String,
    snapshot: Option<String>,
    submitted_by: String,
    submitted_at: DateTime<Utc>,
    approved_by: Option<String>,
    approved_at: Option<DateTime<Utc>>,
    next_review_due: Option<NaiveDate>,
    comment: Option<String>,
    review_comment: Option<String>,
}

impl TryFrom<PlanRow> for PlanVersion {
    type Error = AppError;

    fn try_from(r: PlanRow) -> AppResult<Self> {
        Ok(PlanVersion {
            id: r.id,
            tenant_id: TenantId(r.tenant_id),
            service_id: r.service_id,
            plan_number: r.plan_number.max(1) as u32,
            status: db_enum(&r.status)?,
            snapshot: r.snapshot,
            submitted_by: r.submitted_by,
            submitted_at: r.submitted_at,
            approved_by: r.approved_by,
            approved_at: r.approved_at,
            next_review_due: r.next_review_due,
            comment: r.comment,
            review_comment: r.review_comment,
        })
    }
}

pub struct PgPlanVersionRepository(pub Db);

#[async_trait]
impl PlanVersionRepository for PgPlanVersionRepository {
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        status: Option<PlanStatus>,
    ) -> AppResult<Vec<PlanVersion>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<PlanRow> = sqlx::query_as(
            "select id, tenant_id, service_id, plan_number, status, submitted_by, submitted_at, approved_by, approved_at,
                    next_review_due, comment, review_comment, null::text as snapshot from plan_version
             where tenant_id = $1 and service_id = $2 and ($3::text is null or status = $3)
             order by plan_number desc",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .bind(status.map(|s| s.as_str()))
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(PlanVersion::try_from).collect()
    }

    async fn get(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        with_snapshot: bool,
    ) -> AppResult<Option<PlanVersion>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<PlanRow> = sqlx::query_as(
            "select id, tenant_id, service_id, plan_number, status, submitted_by, submitted_at, approved_by, approved_at,
                    next_review_due, comment, review_comment, case when $3 then snapshot::text end as snapshot from plan_version
             where tenant_id = $1 and id = $2",
        )
        .bind(ctx.tenant_id.0)
        .bind(id)
        .bind(with_snapshot)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        row.map(PlanVersion::try_from).transpose()
    }

    async fn current_approved(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Option<PlanVersion>> {
        Ok(self
            .list_by_service(ctx, service_id, Some(PlanStatus::Approved))
            .await?
            .into_iter()
            .next())
    }

    async fn next_plan_number(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<u32> {
        let mut tx = self.0.begin(ctx).await?;
        let max: Option<i32> = sqlx::query_scalar(
            "select max(plan_number) from plan_version where tenant_id = $1 and service_id = $2",
        )
        .bind(ctx.tenant_id.0)
        .bind(service_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(max.unwrap_or(0).max(0) as u32 + 1)
    }

    async fn insert(&self, ctx: &TenantContext, v: &PlanVersion) -> AppResult<()> {
        let snapshot = v
            .snapshot
            .as_deref()
            .ok_or_else(|| AppError::internal("snapshot missing"))?;
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query(
            "insert into plan_version (id, tenant_id, service_id, plan_number, status, snapshot, submitted_by, submitted_at, comment)
             values ($1, $2, $3, $4, $5, $6::jsonb, $7, $8, $9)",
        )
        .bind(v.id)
        .bind(ctx.tenant_id.0)
        .bind(v.service_id)
        .bind(v.plan_number as i32)
        .bind(v.status.as_str())
        .bind(snapshot)
        .bind(&v.submitted_by)
        .bind(v.submitted_at)
        .bind(&v.comment)
        .execute(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        Ok(())
    }

    async fn save_status(&self, ctx: &TenantContext, v: &PlanVersion) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        if v.status == PlanStatus::Approved {
            sqlx::query(
                "update plan_version set status = 'retired'
                 where tenant_id = $1 and service_id = $2 and status = 'approved' and id <> $3",
            )
            .bind(ctx.tenant_id.0)
            .bind(v.service_id)
            .bind(v.id)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "update plan_version set status = $3, approved_by = $4, approved_at = $5, next_review_due = $6, review_comment = $7
             where tenant_id = $1 and id = $2",
        )
        .bind(ctx.tenant_id.0)
        .bind(v.id)
        .bind(v.status.as_str())
        .bind(&v.approved_by)
        .bind(v.approved_at)
        .bind(v.next_review_due)
        .bind(&v.review_comment)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

// ───────────────────────────── Markdown export ─────────────────────────────

/// Renders a standalone emergency handbook (BSI Notfallhandbuch / NIST ISCP) in Markdown.
pub struct MarkdownPlanRenderer;

fn minutes(m: Option<Minutes>) -> String {
    match m.map(Minutes::get) {
        None => "–".into(),
        Some(v) if v >= 1440 && v % 1440 == 0 => format!("{} d", v / 1440),
        Some(v) if v >= 60 && v % 60 == 0 => format!("{} h", v / 60),
        Some(v) => format!("{v} min"),
    }
}

fn cell(s: Option<&str>) -> String {
    s.unwrap_or("–").replace('|', "\\|").replace('\n', " ")
}

impl PlanRenderer for MarkdownPlanRenderer {
    fn markdown(&self, v: Option<&PlanVersion>, a: &PlanAggregate, lang: Language) -> String {
        let de = lang == Language::De;
        let l = |en: &'static str, de_: &'static str| if de { de_ } else { en };
        let mut out = String::new();
        let s = &a.service;
        let _ = writeln!(
            out,
            "# {}: {}\n",
            l("Disaster Recovery Plan", "Notfallhandbuch"),
            s.name
        );
        match v {
            Some(v) => {
                let _ = writeln!(
                    out,
                    "{} **{}** · {} **{}** · {} {} · {} {}\n",
                    l("Version", "Version"),
                    v.plan_number,
                    l("Status", "Status"),
                    v.status,
                    l("approved", "freigegeben"),
                    v.approved_at
                        .map(|d| d.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| "–".into()),
                    l("next review", "nächste Prüfung"),
                    v.next_review_due
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "–".into()),
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "> **{}** — {}\n",
                    l("DRAFT", "ENTWURF"),
                    l(
                        "generated from the current working data; not approved.",
                        "aus den aktuellen Arbeitsdaten erzeugt; nicht freigegeben."
                    )
                );
            }
        }
        if let Some(d) = &s.description {
            let _ = writeln!(out, "{d}\n");
        }
        let owner = |id: Option<Uuid>| {
            id.and_then(|i| a.person(i))
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "–".into())
        };
        let _ = writeln!(
            out,
            "- {}: {}",
            l("Business owner", "Fachlich verantwortlich"),
            owner(s.business_owner_id)
        );
        let _ = writeln!(
            out,
            "- {}: {}\n",
            l("Technical owner", "Technisch verantwortlich"),
            owner(s.technical_owner_id)
        );

        if let Some(b) = &a.bia {
            let _ = writeln!(
                out,
                "## {}\n",
                l("Recovery objectives (BIA)", "Wiederanlaufparameter (BIA)")
            );
            let _ = writeln!(
                out,
                "| MTPD / MTA | RTO / WAZ | RPO / MTDV |\n|---|---|---|"
            );
            let _ = writeln!(
                out,
                "| {} | {} | {} |\n",
                minutes(Some(b.mtpd)),
                minutes(Some(b.service_rto)),
                minutes(Some(b.service_rpo))
            );
            if let Some(m) = &b.minimum_operating_level {
                let _ = writeln!(
                    out,
                    "**{}:** {m}\n",
                    l("Minimum operating level", "Notbetriebsniveau")
                );
            }
        }

        let _ = writeln!(
            out,
            "## {}\n",
            l("Contacts and roles", "Kontakte und Rollen")
        );
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |\n|---|---|---|---|---|",
            l("Role", "Rolle"),
            l("Person", "Person"),
            l("Phone", "Telefon"),
            l("Alternate contact", "Alternativer Kontakt"),
            l("Deputy", "Vertretung")
        );
        for e in &a.role_assignments {
            let p = e.person.as_ref();
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} |",
                cell(Some(&e.role_name)),
                cell(p.map(|p| p.name.as_str())),
                cell(p.and_then(|p| p.phone.as_deref())),
                cell(p.and_then(|p| p.alternate_contact.as_deref())),
                if e.assignment.is_deputy {
                    l("yes", "ja")
                } else {
                    l("no", "nein")
                }
            );
        }
        let _ = writeln!(out);

        if !a.communication_rules.is_empty() {
            let _ = writeln!(out, "## {}\n", l("Communication", "Kommunikation"));
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} |\n|---|---|---|---|---|",
                l("Trigger", "Auslöser"),
                l("Audience", "Empfänger"),
                l("Channel", "Kanal"),
                l("Responsible", "Verantwortlich"),
                l("Frequency", "Intervall")
            );
            for r in &a.communication_rules {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} |",
                    r.trigger,
                    r.audience,
                    cell(Some(&r.channel)),
                    cell(a.role_name(r.responsible_role_id)),
                    minutes(r.frequency)
                );
            }
            let _ = writeln!(out);
        }

        let objectives = a.objectives();
        for scenario in a.dr_scenarios() {
            let _ = writeln!(
                out,
                "## {}: {}\n",
                l("Scenario", "Szenario"),
                scenario.title
            );
            if let Some(d) = &scenario.description {
                let _ = writeln!(out, "{d}\n");
            }
            for bundle in &a.microservices {
                let m = &bundle.microservice;
                let runbooks: Vec<_> = bundle
                    .runbooks
                    .iter()
                    .filter(|r| r.runbook.scenario_id == scenario.meta.id)
                    .collect();
                if runbooks.is_empty() {
                    continue;
                }
                let objective = crate::features::objectives::domain::effective(
                    &objectives,
                    m.meta.id,
                    Some(scenario.meta.id),
                );
                let _ = writeln!(
                    out,
                    "### {} (RTO {}, RPO {})\n",
                    m.name,
                    minutes(objective.map(|o| o.rto)),
                    minutes(objective.map(|o| o.rpo))
                );
                for rb in runbooks {
                    let _ = writeln!(
                        out,
                        "**{}** — {} {}\n",
                        rb.runbook.title,
                        l("critical path", "kritischer Pfad"),
                        minutes(Some(critical_path(&rb.steps)))
                    );
                    for phase in RunbookPhase::ALL {
                        let steps: Vec<_> = rb.steps.iter().filter(|s| s.phase == *phase).collect();
                        if steps.is_empty() {
                            continue;
                        }
                        let _ = writeln!(
                            out,
                            "_{}_\n",
                            match phase {
                                RunbookPhase::Activation =>
                                    l("Activation & notification", "Alarmierung"),
                                RunbookPhase::Recovery => l("Recovery", "Wiederanlauf"),
                                RunbookPhase::Reconstitution =>
                                    l("Reconstitution", "Wiederherstellung"),
                            }
                        );
                        for st in steps {
                            let owner =
                                st.owner_role_id.and_then(|r| a.role_name(r)).unwrap_or("–");
                            let _ = writeln!(
                                out,
                                "{}. [ ] **{}** ({owner}, {})",
                                st.seq,
                                st.title,
                                minutes(st.expected_duration)
                            );
                            if let Some(i) = &st.instructions {
                                for line in i.lines() {
                                    let _ = writeln!(out, "   {line}");
                                }
                            }
                            if let Some(vf) = &st.verification {
                                let _ = writeln!(out, "   - {}: {vf}", l("Verify", "Prüfen"));
                            }
                            if st.is_decision_point {
                                let auth = st
                                    .requires_authorization_role_id
                                    .and_then(|r| a.role_name(r))
                                    .unwrap_or("–");
                                let _ = writeln!(
                                    out,
                                    "   - {}: {auth}",
                                    l(
                                        "Decision point, authorized by",
                                        "Entscheidungspunkt, freizugeben durch"
                                    )
                                );
                            }
                        }
                        let _ = writeln!(out);
                    }
                }
            }
        }

        let strategies: Vec<_> = a
            .microservices
            .iter()
            .flat_map(|b| b.strategies.iter().map(move |st| (&b.microservice, st)))
            .filter(|(_, st)| st.is_selected)
            .collect();
        if !strategies.is_empty() {
            let _ = writeln!(
                out,
                "## {}\n",
                l(
                    "Mitigations and recovery capability",
                    "Maßnahmen und Wiederanlauffähigkeit"
                )
            );
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} | {} |\n|---|---|---|---|---|---|---|",
                l("Component", "Komponente"),
                l("Scenario", "Szenario"),
                l("Strategy", "Strategie"),
                l("Est. RTO / RPO", "Gesch. RTO / RPO"),
                l("Implementation", "Umsetzung"),
                l("Last tested", "Zuletzt getestet"),
                l("Gap", "Lücke")
            );
            for (m, st) in strategies {
                let scenarios: Vec<&str> = a
                    .scenarios
                    .iter()
                    .filter(|sc| st.covers(sc.meta.id))
                    .map(|sc| sc.title.as_str())
                    .collect();
                let scenario = (!scenarios.is_empty()).then(|| scenarios.join(", "));
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} / {} | {} | {} | {} |",
                    cell(Some(&m.name)),
                    cell(scenario.as_deref()),
                    cell(Some(
                        st.title.as_deref().unwrap_or(st.strategy_type.as_str())
                    )),
                    minutes(Some(st.estimated_rto)),
                    minutes(Some(st.estimated_rpo)),
                    st.implementation_status,
                    st.last_tested_at
                        .map(|d| d.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| "–".into()),
                    cell(st.gap_justification.as_deref())
                );
            }
            let _ = writeln!(out);
        }

        let backups: Vec<_> = a
            .microservices
            .iter()
            .flat_map(|b| b.data_protection.iter().map(move |d| (&b.microservice, d)))
            .collect();
        if !backups.is_empty() {
            let _ = writeln!(out, "## {}\n", l("Data protection", "Datensicherung"));
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} |\n|---|---|---|---|---|",
                l("Component", "Komponente"),
                l("Data store", "Datenspeicher"),
                l("Method", "Verfahren"),
                l("Frequency", "Intervall"),
                l("Offsite / immutable", "Extern / unveränderbar")
            );
            for (m, d) in backups {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} / {} |",
                    cell(Some(&m.name)),
                    cell(Some(&d.data_store)),
                    d.method,
                    minutes(Some(d.frequency)),
                    d.offsite,
                    d.immutable
                );
            }
        }
        out
    }
}
