use std::sync::Arc;

use uuid::Uuid;

use super::domain::{
    Member, MemberPatch, MemberRepository, NewMember, Person, PersonInput, PersonRepository, Role,
    RoleInput, RoleRepository,
};
use crate::shared::kernel::{AppError, AppResult, Page, PageRequest, TenantContext};

pub struct DirectoryUseCases {
    persons: Arc<dyn PersonRepository>,
    roles: Arc<dyn RoleRepository>,
    members: Arc<dyn MemberRepository>,
}

impl DirectoryUseCases {
    pub fn new(
        persons: Arc<dyn PersonRepository>,
        roles: Arc<dyn RoleRepository>,
        members: Arc<dyn MemberRepository>,
    ) -> Self {
        Self {
            persons,
            roles,
            members,
        }
    }

    // Persons

    pub async fn list_persons(
        &self,
        ctx: &TenantContext,
        q: Option<&str>,
        page: PageRequest,
    ) -> AppResult<Page<Person>> {
        self.persons.list(ctx, q, page).await
    }

    pub async fn get_person(&self, ctx: &TenantContext, id: Uuid) -> AppResult<Person> {
        self.persons
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("person"))
    }

    pub async fn create_person(
        &self,
        ctx: &TenantContext,
        input: PersonInput,
    ) -> AppResult<Person> {
        let person = Person::create(ctx, input)?;
        self.persons.insert(ctx, &person).await
    }

    pub async fn update_person(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        if_match: Option<i32>,
        patch: PersonInput,
    ) -> AppResult<Person> {
        let mut person = self.get_person(ctx, id).await?;
        person.meta.check_version(if_match)?;
        person.apply(patch)?;
        self.persons.update(ctx, &person).await
    }

    pub async fn delete_person(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        self.get_person(ctx, id).await?;
        if self.persons.owns_open_action_items(ctx, id).await? {
            return Err(AppError::conflict(
                "the person still owns open action items",
            ));
        }
        self.persons.delete(ctx, id).await
    }

    // Roles

    pub async fn list_roles(&self, ctx: &TenantContext) -> AppResult<Vec<Role>> {
        self.roles.list(ctx).await
    }

    pub async fn create_role(&self, ctx: &TenantContext, input: RoleInput) -> AppResult<Role> {
        let role = Role::create(ctx, input)?;
        self.roles.insert(ctx, &role).await
    }

    pub async fn update_role(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        patch: RoleInput,
    ) -> AppResult<Role> {
        let mut role = self
            .roles
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("role"))?;
        role.apply(patch)?;
        self.roles.update(ctx, &role).await
    }

    pub async fn delete_role(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        let role = self
            .roles
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("role"))?;
        role.ensure_deletable()?;
        self.roles.delete(ctx, id).await
    }

    // Members

    pub async fn list_members(
        &self,
        ctx: &TenantContext,
        page: PageRequest,
    ) -> AppResult<Page<Member>> {
        self.members.list(ctx, page).await
    }

    pub async fn add_member(&self, ctx: &TenantContext, new: NewMember) -> AppResult<Member> {
        let member = Member::create(ctx, new)?;
        self.members.insert(ctx, &member).await
    }

    pub async fn update_member(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        patch: MemberPatch,
    ) -> AppResult<Member> {
        let mut member = self
            .members
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("member"))?;
        member.apply(patch);
        self.members.update(ctx, &member).await
    }

    pub async fn remove_member(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        if self.members.delete(ctx, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("member"))
        }
    }
}
