use std::future::Future;

use academy_models::{
    auth::{AccessToken, AuthError},
    mfa::{MfaRecoveryCode, TotpCode, TotpSetup},
    user::UserIdOrSelf,
};
use thiserror::Error;

pub mod authenticate;
pub mod disable;
pub mod recovery;
pub mod totp_device;

pub trait MfaFeatureService: Send + Sync + 'static {
    /// Create a new disabled TOTP device or reset an existing disabled TOTP
    /// device.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn initialize(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<TotpSetup, MfaInitializeError>> + Send;

    /// Enable a previously created disabled TOTP device and generate an MFA
    /// recovery code.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn enable(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        code: TotpCode,
    ) -> impl Future<Output = Result<MfaRecoveryCode, MfaEnableError>> + Send;

    /// Delete all TOTP devices and invalidate the MFA recovery code.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn disable(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<(), MfaDisableError>> + Send;
}

#[derive(Debug, Error)]
pub enum MfaInitializeError {
    #[error("The user has already enabled mfa.")]
    AlreadyEnabled,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum MfaEnableError {
    #[error("The user has already enabled mfa.")]
    AlreadyEnabled,
    #[error("Mfa has not been initialized.")]
    NotInitialized,
    #[error("The totp code in incorrect.")]
    InvalidCode,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum MfaDisableError {
    #[error("The user has not enabled mfa.")]
    NotEnabled,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
