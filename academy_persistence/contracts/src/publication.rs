use std::future::Future;

use academy_models::{
    publication::{
        PublicationChoice, PublicationChoiceResult, PublicationEpoch, PublicationSettings,
        PublicationSnapshot,
    },
    user::UserId,
};
use thiserror::Error;

pub trait PublicationRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    fn epoch(
        &self,
        txn: &mut Txn,
        enabled: bool,
    ) -> impl Future<Output = anyhow::Result<PublicationEpoch>> + Send;
    fn snapshot(
        &self,
        txn: &mut Txn,
        enabled: bool,
    ) -> impl Future<Output = anyhow::Result<PublicationSnapshot>> + Send;
    fn settings(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<PublicationSettings>>> + Send;
    /// Lock owner and profile; validate current verification, bounded replay and CAS.
    fn choose(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        choice: &PublicationChoice,
        enabled: bool,
        preview_valid: bool,
    ) -> impl Future<Output = Result<PublicationChoiceResult, PublicationWriteError>> + Send;
}

#[derive(Debug, Error)]
pub enum PublicationWriteError {
    #[error("Publication is disabled.")]
    Disabled,
    #[error("The account does not exist.")]
    NotFound,
    #[error("The publication choice conflicts with the current revision or request.")]
    Conflict,
    #[error("A current matching preview is required.")]
    InvalidPreview,
    #[error("Verify the account email before sharing.")]
    Unverified,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
