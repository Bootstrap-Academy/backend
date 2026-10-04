use std::future::Future;

use academy_models::{
    auth::{AccessToken, AuthError, InternalToken},
    publication::{
        PublicationChoice, PublicationChoiceResult, PublicationEpoch, PublicationPreview,
        PublicationSettings, PublicationSnapshot, PublicationWithdrawal,
    },
    user::UserId,
};
use thiserror::Error;

pub trait PublicationFeatureService: Send + Sync + 'static {
    fn support_settings(
        &self,
        token: &AccessToken,
        user_id: UserId,
    ) -> impl Future<Output = Result<PublicationSettings, PublicationError>> + Send;
    fn support_withdraw(
        &self,
        token: &AccessToken,
        user_id: UserId,
        withdrawal: PublicationWithdrawal,
    ) -> impl Future<Output = Result<PublicationChoiceResult, PublicationError>> + Send;
    fn settings(
        &self,
        token: &AccessToken,
    ) -> impl Future<Output = Result<PublicationSettings, PublicationError>> + Send;
    fn preview(
        &self,
        token: &AccessToken,
    ) -> impl Future<Output = Result<PublicationPreview, PublicationError>> + Send;
    fn choose(
        &self,
        token: &AccessToken,
        choice: PublicationChoice,
    ) -> impl Future<Output = Result<PublicationChoiceResult, PublicationError>> + Send;
    fn epoch(
        &self,
        token: &InternalToken,
    ) -> impl Future<Output = Result<PublicationEpoch, PublicationError>> + Send;
    fn snapshot(
        &self,
        token: &InternalToken,
    ) -> impl Future<Output = Result<PublicationSnapshot, PublicationError>> + Send;
}

#[derive(Debug, Error)]
pub enum PublicationError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("Invalid internal authentication.")]
    InternalAuth,
    #[error("Publication is unavailable.")]
    Disabled,
    #[error("The account does not exist.")]
    NotFound,
    #[error("The publication choice conflicts with the current revision or request.")]
    Conflict,
    #[error("A current matching preview is required.")]
    InvalidPreview,
    #[error("Verify the account email before sharing.")]
    Unverified,
    #[error("Only the owner's own sign-in can change the publication.")]
    NotOwnerSignIn,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
