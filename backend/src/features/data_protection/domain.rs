//! Backup and replication configuration per microservice (BSI IT-Grundschutz CON.3
//! Datensicherungskonzept). Needed to prove the RPO.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::features::objectives::domain::{RecoveryObjective, effective};
use crate::shared::kernel::{
    AppResult, Issues, Meta, Minutes, Provenance, TenantContext, str_enum,
};

str_enum! {
    pub enum DataProtectionMethod { Snapshot = "snapshot", LogicalBackup = "logical_backup", Replication = "replication", Pitr = "pitr" }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataProtection {
    pub meta: Meta,
    pub provenance: Provenance,
    pub microservice_id: Uuid,
    pub data_store: String,
    pub method: DataProtectionMethod,
    pub frequency: Minutes,
    pub retention_days: Option<u32>,
    pub offsite: bool,
    pub immutable: bool,
    pub encrypted: bool,
    pub last_restore_test_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default)]
pub struct DataProtectionInput {
    pub data_store: Option<String>,
    pub method: Option<DataProtectionMethod>,
    pub frequency: Option<Minutes>,
    pub retention_days: Option<u32>,
    pub offsite: Option<bool>,
    pub immutable: Option<bool>,
    pub encrypted: Option<bool>,
    pub last_restore_test_at: Option<Option<DateTime<Utc>>>,
}

impl DataProtection {
    pub fn create(
        ctx: &TenantContext,
        microservice_id: Uuid,
        method: DataProtectionMethod,
        frequency: Minutes,
        input: DataProtectionInput,
    ) -> AppResult<Self> {
        let mut d = DataProtection {
            meta: Meta::new(ctx),
            provenance: Provenance::default(),
            microservice_id,
            data_store: String::new(),
            method,
            frequency,
            retention_days: None,
            offsite: false,
            immutable: false,
            encrypted: false,
            last_restore_test_at: None,
        };
        d.apply(input)?;
        Ok(d)
    }

    pub fn apply(&mut self, p: DataProtectionInput) -> AppResult<()> {
        if let Some(v) = p.data_store {
            self.data_store = v.trim().to_owned();
        }
        if let Some(v) = p.method {
            self.method = v;
        }
        if let Some(v) = p.frequency {
            self.frequency = v;
        }
        if p.retention_days.is_some() {
            self.retention_days = p.retention_days;
        }
        if let Some(v) = p.offsite {
            self.offsite = v;
        }
        if let Some(v) = p.immutable {
            self.immutable = v;
        }
        if let Some(v) = p.encrypted {
            self.encrypted = v;
        }
        if let Some(v) = p.last_restore_test_at {
            self.last_restore_test_at = v;
        }
        let mut issues = Issues::new();
        issues.check(
            !self.data_store.is_empty(),
            "REQUIRED",
            "/dataStore",
            "dataStore is required",
        );
        issues.into_result()
    }

    /// True if the backup/replication frequency is within the microservice's default RPO.
    pub fn supports_rpo(&self, objectives: &[RecoveryObjective]) -> Option<bool> {
        effective(objectives, self.microservice_id, None).map(|o| self.frequency <= o.rpo)
    }
}

#[async_trait]
pub trait DataProtectionRepository: Send + Sync {
    async fn list_by_microservice(
        &self,
        ctx: &TenantContext,
        microservice_id: Uuid,
    ) -> AppResult<Vec<DataProtection>>;
    async fn list_by_service(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<DataProtection>>;
    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<DataProtection>>;
    async fn insert(&self, ctx: &TenantContext, d: &DataProtection) -> AppResult<DataProtection>;
    async fn update(&self, ctx: &TenantContext, d: &DataProtection) -> AppResult<DataProtection>;
    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<bool>;
}
