use std::future::Future;

use academy_models::{
    RecaptchaResponse, VerificationCode,
    auth::{AccessToken, AuthError, Login},
    email_address::EmailAddress,
    oauth2::OAuth2RegistrationToken,
    session::DeviceName,
    user::{
        TermsVersion, UserComposite, UserDisplayName, UserIdOrSelf, UserInvoiceInfo, UserName,
        UserPassword, UserProfilePatch,
    },
};
use academy_utils::patch::PatchValue;
use chrono::{DateTime, Utc};
use thiserror::Error;
use user::{UserListQuery, UserListResult};

pub mod email_confirmation;
pub mod update;
pub mod user;

pub trait UserFeatureService: Send + Sync + 'static {
    /// Return all users matching the given query.
    ///
    /// Requires admin privileges.
    fn list_users(
        &self,
        token: &AccessToken,
        query: UserListQuery,
    ) -> impl Future<Output = Result<UserListResult, UserListError>> + Send;

    /// Return the user with the given id.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn get_user(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<UserComposite, UserGetError>> + Send;

    /// Create a new user and logs them in.
    ///
    /// The request must contain the version of the terms and conditions the
    /// user accepted and confirm that the user meets the minimum age.
    fn create_user(
        &self,
        request: UserCreateRequest,
        device_name: Option<DeviceName>,
        recaptcha_response: Option<RecaptchaResponse>,
    ) -> impl Future<Output = Result<Login, UserCreateError>> + Send;

    /// Update a user.
    ///
    /// - Changing the email address will also set `email_verified` to `false`.
    /// - Disabling a user will also log them out.
    /// - A user can never change their own admin status.
    /// - A user can never disable themselves.
    ///
    /// If the authenticated user is not an administrator:
    /// - Only the authenticated user itself can be updated.
    /// - Changing the `name` is rate-limited.
    /// - Changing any of the following fields is not allowed:
    ///   - `enabled`
    ///   - `admin`
    ///   - `email_verified`
    fn update_user(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        request: UserUpdateRequest,
    ) -> impl Future<Output = Result<UserComposite, UserUpdateError>> + Send;

    /// Record that the authenticated user accepted a version of the terms and
    /// conditions.
    ///
    /// Sets `terms_version` and `terms_accepted_at`, and `age_confirmed_at` if
    /// the user has not confirmed their age before. Only the user themselves
    /// can accept the terms and conditions.
    fn accept_terms(
        &self,
        token: &AccessToken,
        request: UserAcceptTermsRequest,
    ) -> impl Future<Output = Result<UserComposite, UserAcceptTermsError>> + Send;

    /// Delete a user.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn delete_user(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<(), UserDeleteError>> + Send;

    /// Request an email with a verification code to verify a user's email
    /// address.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn request_verification_email(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<(), UserRequestVerificationEmailError>> + Send;

    /// Verify a user's email address using the verification code.
    fn verify_email(
        &self,
        code: VerificationCode,
    ) -> impl Future<Output = Result<(), UserVerifyEmailError>> + Send;

    /// Request an email with a verification code to reset a user's password.
    fn request_password_reset(
        &self,
        email: EmailAddress,
        recaptcha_response: Option<RecaptchaResponse>,
    ) -> impl Future<Output = Result<(), UserRequestPasswordResetError>> + Send;

    /// Reset a user's password using the verification code sent via email.
    fn reset_password(
        &self,
        email: EmailAddress,
        code: VerificationCode,
        new_password: UserPassword,
    ) -> impl Future<Output = Result<UserComposite, UserResetPasswordError>> + Send;
}

#[derive(Debug, Error)]
pub enum UserListError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum UserGetError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug)]
pub struct UserCreateRequest {
    pub name: UserName,
    pub display_name: UserDisplayName,
    pub email: EmailAddress,
    pub password: Option<UserPassword>,
    pub oauth2_registration_token: Option<OAuth2RegistrationToken>,
    /// Version of the terms and conditions the user accepted.
    pub terms_version: TermsVersion,
    /// Whether the user confirmed to meet the minimum age. Must be `true`.
    pub age_confirmed: bool,
}

#[derive(Debug, Error)]
pub enum UserCreateError {
    #[error("A user with the same name already exists.")]
    NameConflict,
    #[error("A user with the same email address already exists.")]
    EmailConflict,
    #[error("Invalid recaptcha response")]
    Recaptcha,
    #[error("No login method has been provided.")]
    NoLoginMethod,
    #[error("The user did not confirm to meet the minimum age.")]
    AgeNotConfirmed,
    #[error("The oauth registration token is invalid or has expired.")]
    InvalidOAuthRegistrationToken,
    #[error("The remote user has already been linked.")]
    RemoteAlreadyLinked,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Default)]
pub struct UserUpdateRequest {
    pub user: UserUpdateUserRequest,
    pub profile: UserProfilePatch,
    pub invoice_info: UserInvoiceInfo,
}

#[derive(Debug, Default)]
pub struct UserUpdateUserRequest {
    pub name: PatchValue<UserName>,
    pub email: PatchValue<EmailAddress>,
    pub email_verified: PatchValue<bool>,
    pub password: PatchValue<PasswordUpdate>,
    pub enabled: PatchValue<bool>,
    pub admin: PatchValue<bool>,
}

#[derive(Debug)]
pub enum PasswordUpdate {
    Change(UserPassword),
    Remove,
}

#[derive(Debug, Error)]
pub enum UserUpdateError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    NotFound,
    #[error("A user with the same name already exists.")]
    NameConflict,
    #[error("A user with the same email address already exists.")]
    EmailConflict,
    #[error(
        "The password cannot be removed from the user because they don't have any other login \
         methods."
    )]
    CannotRemovePassword,
    #[error("The user cannot disable their own account.")]
    CannotDisableSelf,
    #[error("The user cannot change their own admin status.")]
    CannotDemoteSelf,
    #[error("The user cannot change their name until {until}.")]
    NameChangeRateLimit { until: DateTime<Utc> },
    #[error("The vat id is invalid.")]
    InvalidVatId,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug)]
pub struct UserAcceptTermsRequest {
    /// Version of the terms and conditions the user accepts.
    pub terms_version: TermsVersion,
    /// Whether the user confirmed to meet the minimum age. Must be `true`.
    pub age_confirmed: bool,
}

#[derive(Debug, Error)]
pub enum UserAcceptTermsError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    NotFound,
    #[error("The user did not confirm to meet the minimum age.")]
    AgeNotConfirmed,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum UserDeleteError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum UserRequestVerificationEmailError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    NotFound,
    #[error("The user's email address has already been verified.")]
    AlreadyVerified,
    #[error("The user does not have an email address.")]
    NoEmail,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum UserVerifyEmailError {
    #[error("The verification code is invalid.")]
    InvalidCode,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum UserRequestPasswordResetError {
    #[error("Invalid recaptcha response")]
    Recaptcha,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum UserResetPasswordError {
    #[error("The email or verification code is invalid.")]
    Failed,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
