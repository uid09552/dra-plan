//! Business impact analysis (NIST SP 800-34 step 2 / BSI 200-4 BIA): impact over time, MTPD (BSI MTA),
//! service RTO (WAZ), service RPO (MTDV) and the minimum operating level (Notbetriebsniveau).

use std::collections::HashSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::shared::kernel::{
    AppResult, ImpactCategory, Issue, Issues, Meta, Minutes, Provenance, Rating, TenantContext,
    non_blank,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ImpactRating {
    pub category: ImpactCategory,
    pub time_window: Minutes,
    pub level: Rating,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bia {
    pub meta: Meta,
    pub provenance: Provenance,
    pub service_id: Uuid,
    pub mtpd: Minutes,
    pub service_rto: Minutes,
    pub service_rpo: Minutes,
    pub minimum_operating_level: Option<String>,
    pub regulatory_requirements: Option<String>,
    pub impact_ratings: Vec<ImpactRating>,
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct BiaInput {
    pub mtpd: Minutes,
    pub service_rto: Minutes,
    pub service_rpo: Minutes,
    pub minimum_operating_level: Option<String>,
    pub regulatory_requirements: Option<String>,
    pub impact_ratings: Vec<ImpactRating>,
}

impl Bia {
    /// Builds the new state of the BIA for `service_id` (PUT semantics: everything is replaced).
    pub fn replace(
        ctx: &TenantContext,
        existing: Option<Bia>,
        service_id: Uuid,
        input: BiaInput,
    ) -> AppResult<Self> {
        let (meta, provenance, approved_at) = match existing {
            Some(b) => (b.meta, b.provenance, b.approved_at),
            None => (Meta::new(ctx), Provenance::default(), None),
        };
        let bia = Bia {
            meta,
            provenance,
            service_id,
            mtpd: input.mtpd,
            service_rto: input.service_rto,
            service_rpo: input.service_rpo,
            minimum_operating_level: non_blank(input.minimum_operating_level),
            regulatory_requirements: non_blank(input.regulatory_requirements),
            impact_ratings: input.impact_ratings,
            approved_at,
        };
        bia.validate()?;
        Ok(bia)
    }

    fn validate(&self) -> AppResult<()> {
        let mut issues = Issues::new();
        if self.service_rto > self.mtpd {
            issues.push(
                Issue::blocking(
                    "SERVICE_RTO_EXCEEDS_MTPD",
                    "the service RTO must not exceed the MTPD",
                )
                .field("/serviceRtoMinutes")
                .standard("NIST SP 800-34 §3.2.1 / BSI 200-4 BIA (WAZ ≤ MTA)"),
            );
        }
        let mut seen = HashSet::new();
        for (i, r) in self.impact_ratings.iter().enumerate() {
            if !seen.insert((r.category, r.time_window)) {
                issues.push(
                    Issue::blocking(
                        "DUPLICATE_IMPACT_RATING",
                        "duplicate rating for category and time window",
                    )
                    .field(&format!("/impactRatings/{i}")),
                );
            }
        }
        issues.into_result()
    }

    /// The shortest time window in which any impact category reaches the tolerance level.
    pub fn derived_mtpd(&self, tolerance: Rating) -> Option<Minutes> {
        self.impact_ratings
            .iter()
            .filter(|r| r.level >= tolerance)
            .map(|r| r.time_window)
            .min()
    }
}

#[async_trait]
pub trait BiaRepository: Send + Sync {
    async fn get(&self, ctx: &TenantContext, service_id: Uuid) -> AppResult<Option<Bia>>;
    /// Inserts (new) or updates (existing, optimistic version check) and replaces all ratings.
    async fn save(&self, ctx: &TenantContext, bia: &Bia, is_new: bool) -> AppResult<Bia>;
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn m(v: u32) -> Minutes {
        Minutes::new(v).expect("valid")
    }

    fn rating(category: ImpactCategory, window: u32, level: i64) -> ImpactRating {
        ImpactRating {
            category,
            time_window: m(window),
            level: Rating::new(level).expect("valid"),
            rationale: None,
        }
    }

    #[test]
    fn rejects_rto_above_mtpd() {
        let input = BiaInput {
            mtpd: m(60),
            service_rto: m(120),
            service_rpo: m(15),
            minimum_operating_level: None,
            regulatory_requirements: None,
            impact_ratings: vec![],
        };
        assert!(Bia::replace(&ctx(), None, Uuid::nil(), input).is_err());
    }

    #[test]
    fn derives_mtpd_from_tolerance() {
        let input = BiaInput {
            mtpd: m(1440),
            service_rto: m(240),
            service_rpo: m(15),
            minimum_operating_level: None,
            regulatory_requirements: None,
            impact_ratings: vec![
                rating(ImpactCategory::Financial, 60, 1),
                rating(ImpactCategory::Financial, 1440, 3),
                rating(ImpactCategory::Reputational, 240, 4),
            ],
        };
        let bia = Bia::replace(&ctx(), None, Uuid::nil(), input).expect("valid");
        assert_eq!(
            bia.derived_mtpd(Rating::new(3).expect("valid")),
            Some(m(240))
        );
    }
}
