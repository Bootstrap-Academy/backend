//! Retained commercial rights have independent records and capabilities. Sharing
//! the facade's authentication dependencies does not merge moderation and money.
use crate::RecipientCredentials;
use academy_models::auth::{AccessToken, InternalToken};
use academy_models::purchase::PurchaseStatus;
use serde_json::Value;
use std::future::Future;

pub struct CommercialCredentials {
    pub recipient: RecipientCredentials,
    pub claim_key: Option<String>,
}

pub trait CommercialFeatureService: Send + Sync + 'static {
    fn commercial_document_inventory(
        &self,
        credentials: CommercialCredentials,
    ) -> impl Future<Output = anyhow::Result<academy_models::commercial_document::DocumentInventory>>
    + Send;
    fn commercial_purchase_status(
        &self,
        credentials: CommercialCredentials,
        offer: uuid::Uuid,
    ) -> impl Future<Output = anyhow::Result<PurchaseStatus>> + Send;
    fn commercial_learning(
        &self,
        key: &str,
        operation: &str,
        body: Value,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn commercial_recipient(
        &self,
        credentials: CommercialCredentials,
        operation: &str,
        body: Value,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn commercial_admin(
        &self,
        access: &AccessToken,
        operation: &str,
        body: Value,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn commercial_internal(
        &self,
        token: &InternalToken,
        operation: &str,
        body: Value,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    fn commercial_admin_document(
        &self,
        access: &AccessToken,
        case_id: uuid::Uuid,
        kind: &str,
        id: &str,
        variant: &str,
    ) -> impl Future<Output = anyhow::Result<Vec<u8>>> + Send;
    fn commercial_document(
        &self,
        credentials: CommercialCredentials,
        kind: &str,
        id: &str,
        variant: &str,
    ) -> impl Future<Output = anyhow::Result<Vec<u8>>> + Send;
}
