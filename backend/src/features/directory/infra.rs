use async_trait::async_trait;
use sqlx::FromRow;
use uuid::Uuid;

use super::domain::{Member, MemberRepository, Person, PersonRepository, Role, RoleRepository};
use crate::shared::infra::{Db, MetaRow, db_enum, like_pattern, require_updated, write_err};
use crate::shared::kernel::{AppResult, Page, PageRequest, TenantContext};

// ───────────────────────────── Person ─────────────────────────────

#[derive(FromRow)]
struct PersonRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    name: String,
    email: Option<String>,
    phone: Option<String>,
    alternate_contact: Option<String>,
    team: Option<String>,
}

impl From<PersonRow> for Person {
    fn from(r: PersonRow) -> Self {
        Person {
            meta: r.meta.into(),
            name: r.name,
            email: r.email,
            phone: r.phone,
            alternate_contact: r.alternate_contact,
            team: r.team,
        }
    }
}

pub struct PgPersonRepository(pub Db);

#[async_trait]
impl PersonRepository for PgPersonRepository {
    async fn list(
        &self,
        ctx: &TenantContext,
        query: Option<&str>,
        page: PageRequest,
    ) -> AppResult<Page<Person>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<PersonRow> = sqlx::query_as(
            "select * from person
             where tenant_id = $1
               and ($2::text is null or name ilike $2 or email ilike $2 or team ilike $2)
             order by name, id limit $3 offset $4",
        )
        .bind(ctx.tenant_id.0)
        .bind(query.map(like_pattern))
        .bind(page.fetch_limit())
        .bind(page.offset)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Page::from_overfetch(
            rows.into_iter().map(Person::from).collect(),
            page,
        ))
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Person>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<PersonRow> =
            sqlx::query_as("select * from person where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(row.map(Person::from))
    }

    async fn get_many(&self, ctx: &TenantContext, ids: &[Uuid]) -> AppResult<Vec<Person>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<PersonRow> = sqlx::query_as(
            "select * from person where tenant_id = $1 and id = any($2) order by name, id",
        )
        .bind(ctx.tenant_id.0)
        .bind(ids)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows.into_iter().map(Person::from).collect())
    }

    async fn insert(&self, ctx: &TenantContext, p: &Person) -> AppResult<Person> {
        let mut tx = self.0.begin(ctx).await?;
        let row: PersonRow = sqlx::query_as(
            "insert into person (id, tenant_id, name, email, phone, alternate_contact, team, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $8) returning *",
        )
        .bind(p.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(&p.name)
        .bind(&p.email)
        .bind(&p.phone)
        .bind(&p.alternate_contact)
        .bind(&p.team)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        Ok(row.into())
    }

    async fn update(&self, ctx: &TenantContext, p: &Person) -> AppResult<Person> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<PersonRow> = sqlx::query_as(
            "update person set name = $4, email = $5, phone = $6, alternate_contact = $7, team = $8,
                    version = version + 1, updated_at = now(), updated_by = $9
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(p.meta.id)
        .bind(p.meta.version)
        .bind(&p.name)
        .bind(&p.email)
        .bind(&p.phone)
        .bind(&p.alternate_contact)
        .bind(&p.team)
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        Ok(require_updated(row)?.into())
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from person where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn owns_open_action_items(&self, ctx: &TenantContext, id: Uuid) -> AppResult<bool> {
        let mut tx = self.0.begin(ctx).await?;
        let owns: bool = sqlx::query_scalar(
            "select exists(select 1 from action_item
                           where tenant_id = $1 and owner_person_id = $2 and status in ('open', 'in_progress'))",
        )
        .bind(ctx.tenant_id.0)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(owns)
    }
}

// ───────────────────────────── Role ─────────────────────────────

#[derive(FromRow)]
struct RoleRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    name: String,
    description: Option<String>,
    is_default: bool,
}

impl From<RoleRow> for Role {
    fn from(r: RoleRow) -> Self {
        Role {
            meta: r.meta.into(),
            name: r.name,
            description: r.description,
            is_default: r.is_default,
        }
    }
}

pub struct PgRoleRepository(pub Db);

#[async_trait]
impl RoleRepository for PgRoleRepository {
    async fn list(&self, ctx: &TenantContext) -> AppResult<Vec<Role>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<RoleRow> = sqlx::query_as(
            "select * from role where tenant_id = $1 order by is_default desc, name",
        )
        .bind(ctx.tenant_id.0)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows.into_iter().map(Role::from).collect())
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Role>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<RoleRow> =
            sqlx::query_as("select * from role where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(row.map(Role::from))
    }

    async fn insert(&self, ctx: &TenantContext, r: &Role) -> AppResult<Role> {
        let mut tx = self.0.begin(ctx).await?;
        let row: RoleRow = sqlx::query_as(
            "insert into role (id, tenant_id, name, description, is_default, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $6) returning *",
        )
        .bind(r.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(&r.name)
        .bind(&r.description)
        .bind(r.is_default)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        Ok(row.into())
    }

    async fn update(&self, ctx: &TenantContext, r: &Role) -> AppResult<Role> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<RoleRow> = sqlx::query_as(
            "update role set name = $4, description = $5, version = version + 1, updated_at = now(), updated_by = $6
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(r.meta.id)
        .bind(r.meta.version)
        .bind(&r.name)
        .bind(&r.description)
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        Ok(require_updated(row)?.into())
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let mut tx = self.0.begin(ctx).await?;
        sqlx::query("delete from role where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

// ───────────────────────────── Member ─────────────────────────────

#[derive(FromRow)]
struct MemberRow {
    #[sqlx(flatten)]
    meta: MetaRow,
    user_ref: String,
    email: Option<String>,
    display_name: Option<String>,
    tenant_role: String,
    person_id: Option<Uuid>,
}

impl TryFrom<MemberRow> for Member {
    type Error = crate::shared::kernel::AppError;

    fn try_from(r: MemberRow) -> AppResult<Self> {
        Ok(Member {
            meta: r.meta.into(),
            user_ref: r.user_ref,
            email: r.email,
            display_name: r.display_name,
            tenant_role: db_enum(&r.tenant_role)?,
            person_id: r.person_id,
        })
    }
}

pub struct PgMemberRepository(pub Db);

#[async_trait]
impl MemberRepository for PgMemberRepository {
    async fn list(&self, ctx: &TenantContext, page: PageRequest) -> AppResult<Page<Member>> {
        let mut tx = self.0.begin(ctx).await?;
        let rows: Vec<MemberRow> = sqlx::query_as(
            "select * from tenant_member where tenant_id = $1 order by user_ref, id limit $2 offset $3",
        )
        .bind(ctx.tenant_id.0)
        .bind(page.fetch_limit())
        .bind(page.offset)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let items = rows
            .into_iter()
            .map(Member::try_from)
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Page::from_overfetch(items, page))
    }

    async fn get(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Option<Member>> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<MemberRow> =
            sqlx::query_as("select * from tenant_member where tenant_id = $1 and id = $2")
                .bind(ctx.tenant_id.0)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map(Member::try_from).transpose()
    }

    async fn insert(&self, ctx: &TenantContext, m: &Member) -> AppResult<Member> {
        let mut tx = self.0.begin(ctx).await?;
        let row: MemberRow = sqlx::query_as(
            "insert into tenant_member (id, tenant_id, user_ref, email, display_name, tenant_role, person_id, created_by, updated_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $8) returning *",
        )
        .bind(m.meta.id)
        .bind(ctx.tenant_id.0)
        .bind(&m.user_ref)
        .bind(&m.email)
        .bind(&m.display_name)
        .bind(m.tenant_role.as_str())
        .bind(m.person_id)
        .bind(ctx.actor())
        .fetch_one(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        row.try_into()
    }

    async fn update(&self, ctx: &TenantContext, m: &Member) -> AppResult<Member> {
        let mut tx = self.0.begin(ctx).await?;
        let row: Option<MemberRow> = sqlx::query_as(
            "update tenant_member set tenant_role = $4, person_id = $5,
                    version = version + 1, updated_at = now(), updated_by = $6
             where tenant_id = $1 and id = $2 and version = $3 returning *",
        )
        .bind(ctx.tenant_id.0)
        .bind(m.meta.id)
        .bind(m.meta.version)
        .bind(m.tenant_role.as_str())
        .bind(m.person_id)
        .bind(ctx.actor())
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_err)?;
        tx.commit().await?;
        require_updated(row)?.try_into()
    }

    async fn delete(&self, ctx: &TenantContext, id: Uuid) -> AppResult<bool> {
        let mut tx = self.0.begin(ctx).await?;
        let result = sqlx::query("delete from tenant_member where tenant_id = $1 and id = $2")
            .bind(ctx.tenant_id.0)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }
}
