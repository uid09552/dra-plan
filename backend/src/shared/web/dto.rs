//! DTO building blocks shared by all feature APIs (`ResourceMeta`, `ContentMeta` in the OpenAPI spec).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use uuid::Uuid;

use crate::shared::kernel::{Meta, Origin, Provenance, TenantId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaDto {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
}

impl From<Meta> for MetaDto {
    fn from(m: Meta) -> Self {
        MetaDto {
            id: m.id,
            tenant_id: m.tenant_id.0,
            version: m.version,
            created_at: m.created_at,
            created_by: m.created_by,
            updated_at: m.updated_at,
            updated_by: m.updated_by,
        }
    }
}

impl From<MetaDto> for Meta {
    fn from(m: MetaDto) -> Self {
        Meta {
            id: m.id,
            tenant_id: TenantId(m.tenant_id),
            version: m.version,
            created_at: m.created_at,
            created_by: m.created_by,
            updated_at: m.updated_at,
            updated_by: m.updated_by,
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceDto {
    #[serde_as(as = "DisplayFromStr")]
    pub origin: Origin,
    pub ai_suggestion_id: Option<Uuid>,
}

impl From<Provenance> for ProvenanceDto {
    fn from(p: Provenance) -> Self {
        ProvenanceDto {
            origin: p.origin,
            ai_suggestion_id: p.ai_suggestion_id,
        }
    }
}

impl From<ProvenanceDto> for Provenance {
    fn from(p: ProvenanceDto) -> Self {
        Provenance {
            origin: p.origin,
            ai_suggestion_id: p.ai_suggestion_id,
        }
    }
}

/// Serde helper for nullable merge-patch fields: absent → `None`, `null` → `Some(None)`.
pub mod nullable {
    pub use serde_with::rust::double_option as field;
}
