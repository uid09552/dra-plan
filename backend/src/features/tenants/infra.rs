use async_trait::async_trait;
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{Tenant, TenantRepository, TenantSettings};
use crate::features::directory::domain::NewMember;
use crate::shared::auth::TenantLookup;
use crate::shared::infra::{Db, MetaRow, db_enum, require_updated, write_err};
use crate::shared::kernel::{
    AppError, AppResult, Minutes, Page, PageRequest, Rating, TenantContext, TenantId,
};

#[derive(FromRow)]
struct TenantRow {
    id: Uuid,
    version: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    created_by: String,
    updated_at: chrono::DateTime<chrono::Utc>,
    updated_by: String,
    name: String,
    slug: String,
    review_interval_months: i32,
    impact_categories: Vec<String>,
    impact_time_windows_minutes: Vec<i32>,
    impact_tolerance_level: i32,
    default_language: String,
    ai_enabled: bool,
}

impl TryFrom<TenantRow> for Tenant {
    type Error = AppError;

    fn try_from(r: TenantRow) -> AppResult<Self> {
        Ok(Tenant {
            meta: MetaRow {
                id: r.id,
                tenant_id: r.id,
                version: r.version,
                created_at: r.created_at,
                created_by: r.created_by,
                updated_at: r.updated_at,
                updated_by: r.updated_by,
            }
            .into(),
            name: r.name,
            slug: r.slug,
            settings: TenantSettings {
                review_interval_months: u32::try_from(r.review_interval_months)
                    .map_err(AppError::internal)?,
                impact_categories: r
                    .impact_categories
                    .iter()
                    .map(|c| db_enum(c))
                    .collect::<AppResult<_>>()?,
                impact_time_windows: r
                    .impact_time_windows_minutes
                    .into_iter()
                    .map(Minutes::from_db)
                    .collect::<AppResult<_>>()?,
                impact_tolerance_level: Rating::new(i64::from(r.impact_tolerance_level))
                    .map_err(AppError::internal)?,
                default_language: db_enum(&r.default_language)?,
                ai_enabled: r.ai_enabled,
            },
        })
    }
}

fn categories(t: &Tenant) -> Vec<&'static str> {
    t.settings
        .impact_categories
        .iter()
        .map(|c| c.as_str())
        .collect()
}

fn windows(t: &Tenant) -> Vec<i32> {
    t.settings
        .impact_time_windows
        .iter()
        .map(|m| m.to_db())
        .collect()
}

pub struct PgTenantRepository(pub Db);

#[async_trait]
impl TenantRepository for PgTenantRepository {
    async fn list_for_user(&self, user: &str, page: PageRequest) -> AppResult<Page<Tenant>> {
        let mut tx = self.0.begin_system(user).await?;
        let rows: Vec<TenantRow> = sqlx::query_as(
            "select t.* from tenant t
             where exists (select 1 from tenant_member m where m.tenant_id = t.id and m.user_ref = $1)
             order by t.name, t.id limit $2 offset $3",
        )
        .bind(user)
        .bind(page.fetch_limit())
        .bind(page.offset)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let items = rows
            .into_iter()
            .map(Tenant::try_from)
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Page::from_overfetch(items, page))
    }

    async fn create(
        &self,
        t: &Tenant,
        admin: &NewMember,
        default_roles: &[(&str, &str)],
    ) -> AppResult<Tenant> {
        let actor = &t.meta.created_by;
        let mut tx = self.0.begin_system(actor).await?;
        let row: TenantRow = sqlx::query_as(
            "insert into tenant (id, name, slug, review_interval_months, impact_categories, impact_time_windows_minutes,
                                 impact_tolerance_level, default_language, ai_enabled, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10) returning *",
        )
        .bind(t.meta.id)
        .bind(&t.name)
        .bind(&t.slug)
        .bind(t.settings.review_interval_months as i32)
        .bind(categories(t))
        .bind(windows(t))
        .bind(i32::from(t.settings.impact_tolerance_level.get()))
        .bind(t.settings.default_language.as_str())
        .bind(t.settings.ai_enabled)
        .bind(actor)
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;

        Db::scope_to_tenant(&mut tx, t.id()).await?;
        insert_member(&mut tx, t.id(), admin, actor).await?;
        for (name, description) in default_roles {
            sqlx::query(
                "insert into role (id, tenant_id, name, description, is_default, created_by, updated_by)
                 values ($1, $2, $3, $4, true, $5, $5)",
            )
            .bind(Uuid::new_v4())
            .bind(t.meta.id)
            .bind(name)
            .bind(description)
            .bind(actor)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        row.try_into()
    }

    async fn find_by_slug(&self, slug: &str) -> AppResult<Option<Tenant>> {
        let mut tx = self.0.begin_system("system").await?;
        let row: Option<TenantRow> = sqlx::query_as("select * from tenant where slug = $1")
            .bind(slug)
            .fetch_optional(&mut *tx)
            .await?;
        tx.commit().await?;
        row.map(Tenant::try_from).transpose()
    }

    async fn get(&self, ctx: &TenantContext) -> AppResult<Option<Tenant>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<TenantRow> = sqlx::query_as("select * from tenant where id = $1")
            .bind(ctx.tenant_id.0)
            .fetch_optional(&mut *tx)
            .await?;
        tx.commit().await?;
        row.map(Tenant::try_from).transpose()
    }

    async fn update(&self, ctx: &TenantContext, t: &Tenant) -> AppResult<Tenant> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<TenantRow> = sqlx::query_as(
            "update tenant set name = $3, slug = $4, review_interval_months = $5, impact_categories = $6,
                    impact_time_windows_minutes = $7, impact_tolerance_level = $8, default_language = $9,
                    ai_enabled = $10, version = version + 1, updated_at = now(), updated_by = $11
             where id = $1 and version = $2 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(t.meta.version)
        .bind(&t.name)
        .bind(&t.slug)
        .bind(t.settings.review_interval_months as i32)
        .bind(categories(t))
        .bind(windows(t))
        .bind(i32::from(t.settings.impact_tolerance_level.get()))
        .bind(t.settings.default_language.as_str())
        .bind(t.settings.ai_enabled)
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }

    async fn delete(&self, ctx: &TenantContext) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from tenant where id = $1")
            .bind(ctx.tenant_id.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn ensure_member(&self, tenant_id: TenantId, member: &NewMember) -> AppResult<()> {
        let mut tx = self.0.begin_system(&member.user_ref).await?;
        Db::scope_to_tenant(&mut tx, tenant_id).await?;
        let exists: bool = sqlx::query_scalar(
            "select exists(select 1 from tenant_member where tenant_id = $1 and user_ref = $2)",
        )
        .bind(tenant_id.0)
        .bind(&member.user_ref)
        .fetch_one(&mut *tx)
        .await?;
        if !exists {
            insert_member(&mut tx, tenant_id, member, &member.user_ref).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn insert_member(
    tx: &mut crate::shared::infra::Tx,
    tenant_id: TenantId,
    m: &NewMember,
    actor: &str,
) -> AppResult<()> {
    sqlx::query(
        "insert into tenant_member (id, tenant_id, user_ref, email, display_name, tenant_role, person_id, created_by, updated_by)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id.0)
    .bind(&m.user_ref)
    .bind(&m.email)
    .bind(&m.display_name)
    .bind(m.tenant_role.as_str())
    .bind(m.person_id)
    .bind(actor)
    .execute(&mut **tx)
    .await
    .map_err(write_err)?;
    Ok(())
}

#[async_trait]
impl TenantLookup for PgTenantRepository {
    async fn find_tenant_id(&self, slug_or_id: &str) -> AppResult<Option<TenantId>> {
        let mut tx = self.0.begin_system("system").await?;
        let id: Option<Uuid> = match Uuid::parse_str(slug_or_id) {
            Ok(id) => {
                sqlx::query_scalar("select id from tenant where id = $1")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?
            }
            Err(_) => {
                sqlx::query_scalar("select id from tenant where slug = $1")
                    .bind(slug_or_id)
                    .fetch_optional(&mut *tx)
                    .await?
            }
        };
        tx.commit().await?;
        Ok(id.map(TenantId))
    }
}
