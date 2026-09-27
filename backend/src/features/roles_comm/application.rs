use std::sync::Arc;

use uuid::Uuid;

use super::domain::{
    CommunicationAudience, CommunicationRule, CommunicationRuleInput, CommunicationRuleRepository,
    CommunicationTrigger, RoleAssignment, RoleAssignmentInput, RoleAssignmentRepository,
};
use crate::features::it_services::application::ItServiceUseCases;
use crate::shared::kernel::{AppError, AppResult, TenantContext};

pub struct RolesCommUseCases {
    assignments: Arc<dyn RoleAssignmentRepository>,
    rules: Arc<dyn CommunicationRuleRepository>,
    services: Arc<ItServiceUseCases>,
}

impl RolesCommUseCases {
    pub fn new(
        assignments: Arc<dyn RoleAssignmentRepository>,
        rules: Arc<dyn CommunicationRuleRepository>,
        services: Arc<ItServiceUseCases>,
    ) -> Self {
        Self {
            assignments,
            rules,
            services,
        }
    }

    pub async fn list_assignments(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<RoleAssignment>> {
        self.services.require(ctx, service_id).await?;
        self.assignments.list_by_service(ctx, service_id).await
    }

    pub async fn assign(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        role_id: Uuid,
        person_id: Uuid,
        input: RoleAssignmentInput,
    ) -> AppResult<RoleAssignment> {
        self.services.require(ctx, service_id).await?;
        let a = RoleAssignment::create(ctx, service_id, role_id, person_id, input)?;
        self.assignments.insert(ctx, &a).await
    }

    pub async fn update_assignment(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        patch: RoleAssignmentInput,
    ) -> AppResult<RoleAssignment> {
        let mut a = self
            .assignments
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("role assignment"))?;
        a.apply(patch)?;
        self.assignments.update(ctx, &a).await
    }

    pub async fn delete_assignment(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        if self.assignments.delete(ctx, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("role assignment"))
        }
    }

    pub async fn list_rules(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
    ) -> AppResult<Vec<CommunicationRule>> {
        self.services.require(ctx, service_id).await?;
        self.rules.list_by_service(ctx, service_id).await
    }

    pub async fn create_rule(
        &self,
        ctx: &TenantContext,
        service_id: Uuid,
        trigger: CommunicationTrigger,
        audience: CommunicationAudience,
        responsible_role_id: Uuid,
        input: CommunicationRuleInput,
    ) -> AppResult<CommunicationRule> {
        self.services.require(ctx, service_id).await?;
        let r = CommunicationRule::create(
            ctx,
            service_id,
            trigger,
            audience,
            responsible_role_id,
            input,
        )?;
        self.rules.insert(ctx, &r).await
    }

    pub async fn update_rule(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        patch: CommunicationRuleInput,
    ) -> AppResult<CommunicationRule> {
        let mut r = self
            .rules
            .get(ctx, id)
            .await?
            .ok_or(AppError::NotFound("communication rule"))?;
        r.apply(patch)?;
        self.rules.update(ctx, &r).await
    }

    pub async fn delete_rule(&self, ctx: &TenantContext, id: Uuid) -> AppResult<()> {
        if self.rules.delete(ctx, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("communication rule"))
        }
    }
}
