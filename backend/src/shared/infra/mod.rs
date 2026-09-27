//! Database access shared by all feature adapters.

use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;

use crate::shared::kernel::{
    AppError, AppResult, Issue, Meta, Minutes, Origin, Provenance, Rating, TenantContext, TenantId,
};

pub type Tx = Transaction<'static, Postgres>;

/// Connection pool plus helpers that open tenant-scoped transactions.
#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(url: &SecretString, max_connections: u32) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url.expose_secret())
            .await?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub async fn ping(&self) -> AppResult<()> {
        sqlx::query("select 1").execute(&self.pool).await?;
        Ok(())
    }

    /// Adds missing default roles to all tenants (or the one with `slug`). Returns the number added.
    pub async fn seed_default_roles(
        &self,
        slug: Option<&str>,
        roles: &[(&str, &str)],
    ) -> AppResult<u64> {
        let mut tx = self.begin_system("seed-catalog").await?;
        let tenants: Vec<Uuid> =
            sqlx::query_scalar("select id from tenant where $1::text is null or slug = $1")
                .bind(slug)
                .fetch_all(&mut *tx)
                .await?;
        if slug.is_some() && tenants.is_empty() {
            return Err(AppError::NotFound("tenant"));
        }
        let mut added = 0;
        for tenant in tenants {
            Self::scope_to_tenant(&mut tx, TenantId(tenant)).await?;
            for (name, description) in roles {
                added += sqlx::query(
                    "insert into role (id, tenant_id, name, description, is_default, created_by, updated_by)
                     values ($1, $2, $3, $4, true, 'seed-catalog', 'seed-catalog') on conflict (tenant_id, name) do nothing",
                )
                .bind(Uuid::new_v4())
                .bind(tenant)
                .bind(name)
                .bind(description)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            }
        }
        tx.commit().await?;
        Ok(added)
    }

    /// Opens a transaction scoped to the request's tenant. Sets `app.tenant_id` (row-level
    /// security) and `app.actor` (audit trigger) for the transaction only.
    pub async fn begin(&self, ctx: &TenantContext) -> AppResult<Tx> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "select set_config('app.tenant_id', $1, true), set_config('app.actor', $2, true)",
        )
        .bind(ctx.tenant_id.0.to_string())
        .bind(ctx.actor())
        .execute(&mut *tx)
        .await?;
        Ok(tx)
    }

    /// Opens a transaction without a tenant scope, for tenant-independent operations
    /// (membership lookup, tenant creation). Only tables without row-level security are visible.
    pub async fn begin_system(&self, actor: &str) -> AppResult<Tx> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select set_config('app.actor', $1, true)")
            .bind(actor)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }

    /// Switches an open system transaction to a tenant scope (e.g. right after creating a tenant).
    pub async fn scope_to_tenant(tx: &mut Tx, tenant_id: TenantId) -> AppResult<()> {
        sqlx::query("select set_config('app.tenant_id', $1, true)")
            .bind(tenant_id.0.to_string())
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &error {
            let constraint = db.constraint().unwrap_or("unknown").to_owned();
            match db.code().as_deref() {
                Some("23505") => {
                    return AppError::Conflict(format!(
                        "duplicate value (constraint `{constraint}`)"
                    ));
                }
                Some("23503") => {
                    return AppError::Conflict(format!(
                        "the record is still referenced by other records (constraint `{constraint}`)"
                    ));
                }
                Some("23514") | Some("23502") => {
                    return AppError::Validation(vec![Issue::blocking(
                        "CONSTRAINT_VIOLATION",
                        format!("value violates constraint `{constraint}`"),
                    )]);
                }
                Some("40001") | Some("40P01") => {
                    return AppError::Conflict("concurrent modification, please retry".into());
                }
                _ => {}
            }
        }
        tracing::error!(error = %error, "database error");
        AppError::Internal("database error".into())
    }
}

/// Maps errors of INSERT/UPDATE statements: a foreign-key violation there means a referenced
/// record does not exist (in this tenant), which is a validation problem rather than a conflict.
pub fn write_err(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db) = &error {
        if db.code().as_deref() == Some("23503") {
            let constraint = db.constraint().unwrap_or("unknown");
            return AppError::Validation(vec![Issue::blocking(
                "REFERENCE_NOT_FOUND",
                format!("a referenced record does not exist (constraint `{constraint}`)"),
            )]);
        }
    }
    error.into()
}

/// Metadata columns common to all resource tables (`#[sqlx(flatten)]`).
#[derive(Debug, FromRow)]
pub struct MetaRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
}

impl From<MetaRow> for Meta {
    fn from(r: MetaRow) -> Self {
        Meta {
            id: r.id,
            tenant_id: TenantId(r.tenant_id),
            version: r.version,
            created_at: r.created_at,
            created_by: r.created_by,
            updated_at: r.updated_at,
            updated_by: r.updated_by,
        }
    }
}

/// Provenance columns of content tables (`#[sqlx(flatten)]`).
#[derive(Debug, FromRow)]
pub struct ProvenanceRow {
    pub origin: String,
    pub ai_suggestion_id: Option<Uuid>,
}

impl TryFrom<ProvenanceRow> for Provenance {
    type Error = AppError;

    fn try_from(r: ProvenanceRow) -> AppResult<Self> {
        Ok(Provenance {
            origin: db_enum::<Origin>(&r.origin)?,
            ai_suggestion_id: r.ai_suggestion_id,
        })
    }
}

/// Parses an enum stored in the database. Unknown values mean corrupt data.
pub fn db_enum<T: FromStr>(value: &str) -> AppResult<T> {
    value
        .parse::<T>()
        .map_err(|_| AppError::internal(format!("unexpected enum value in storage: {value}")))
}

pub fn db_enum_opt<T: FromStr>(value: Option<&str>) -> AppResult<Option<T>> {
    value.map(db_enum::<T>).transpose()
}

pub fn db_minutes(value: Option<i32>) -> AppResult<Option<Minutes>> {
    value.map(Minutes::from_db).transpose()
}

pub fn db_rating(value: Option<i32>) -> AppResult<Option<Rating>> {
    value
        .map(|v| Rating::new(i64::from(v)))
        .transpose()
        .map_err(AppError::internal)
}

pub fn opt_minutes(value: Option<Minutes>) -> Option<i32> {
    value.map(Minutes::to_db)
}

/// Escapes `%`, `_` and `\` for use inside an ILIKE pattern.
pub fn like_pattern(q: &str) -> String {
    let escaped = q
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// Update statements check the expected version; zero affected rows means a concurrent change.
pub fn require_updated<T>(row: Option<T>) -> AppResult<T> {
    row.ok_or(AppError::PreconditionFailed)
}
