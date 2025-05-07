use std::sync::Arc;

use academy_core_user_contracts::{
    PasswordUpdate, UserCreateError, UserCreateRequest, UserDeleteError, UserFeatureService,
    UserGetError, UserListError, UserRequestPasswordResetError, UserRequestVerificationEmailError,
    UserResetPasswordError, UserUpdateError, UserUpdateRequest, UserUpdateUserRequest,
    UserVerifyEmailError, UserVerifyNewsletterSubscriptionError,
    user::{UserListQuery, UserListResult},
};
use academy_models::{
    RecaptchaResponse, VerificationCode,
    email_address::EmailAddress,
    oauth2::OAuth2RegistrationToken,
    session::DeviceName,
    user::{
        UserBio, UserCity, UserCountry, UserDisplayName, UserFirstName, UserInvoiceInfo,
        UserLastName, UserName, UserPassword, UserProfilePatch, UserStreet, UserTags, UserVatId,
        UserZipCode,
    },
};
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::oauth2::RemoteAlreadyLinkedError;
use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{
        PermissionDeniedError, RecaptchaFailedError, auth_error, auth_error_docs,
        internal_server_error, internal_server_error_docs,
    },
    extractors::{auth::ApiToken, user_agent::UserAgent},
    models::{
        ApiPaginationSlice, OkResponse, StringOption,
        session::ApiLogin,
        user::{ApiUser, ApiUserFilter, ApiUserIdOrSelf, ApiUserPasswordOrEmpty, PathUserIdOrSelf},
    },
};

pub const TAG: &str = "User";

pub fn router(service: Arc<impl UserFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/auth/users",
            routing::get_with(list, list_docs).post_with(create, create_docs),
        )
        .api_route(
            "/auth/users/{user_id}",
            routing::get_with(get, get_docs)
                .patch_with(update, update_docs)
                .delete_with(delete, delete_docs),
        )
        .api_route(
            "/auth/users/{user_id}/email",
            routing::post_with(request_verification_email, request_verification_email_docs)
                .put_with(verify_email, verify_email_docs),
        )
        .api_route(
            "/auth/users/{user_id}/newsletter",
            routing::put_with(
                verify_newsletter_subscription,
                verify_newsletter_subscription_docs,
            ),
        )
        .api_route(
            "/auth/password_reset",
            routing::post_with(request_password_reset, request_password_reset_docs)
                .put_with(reset_password, reset_password_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

#[derive(Serialize, JsonSchema)]
struct ListResult {
    /// The total number of users matching the given query
    total: u64,
    /// The paginated list of users matching the given query
    users: Vec<ApiUser>,
}

async fn list(
    user_service: State<Arc<impl UserFeatureService>>,
    token: ApiToken,
    Query(pagination): Query<ApiPaginationSlice>,
    Query(filter): Query<ApiUserFilter>,
) -> Response {
    match user_service
        .list_users(
            &token.0,
            UserListQuery {
                filter: filter.into(),
                pagination: pagination.into(),
            },
        )
        .await
    {
        Ok(UserListResult {
            total,
            user_composites: users,
        }) => Json(ListResult {
            total,
            users: users.into_iter().map(Into::into).collect(),
        })
        .into_response(),
        Err(UserListError::Auth(err)) => auth_error(err),
        Err(UserListError::Other(err)) => internal_server_error(err),
    }
}

fn list_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return all users matching the given query.")
        .add_response::<ListResult>(StatusCode::OK, None)
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

async fn get(
    user_service: State<Arc<impl UserFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
) -> Response {
    match user_service.get_user(&token.0, user_id.into()).await {
        Ok(user) => Json(ApiUser::from(user)).into_response(),
        Err(UserGetError::NotFound) => UserNotFoundError.into_response(),
        Err(UserGetError::Auth(err)) => auth_error(err),
        Err(UserGetError::Other(err)) => internal_server_error(err),
    }
}

fn get_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the user with the given id.")
        .add_response::<ApiUser>(StatusCode::OK, None)
        .add_error::<UserNotFoundError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct CreateRequest {
    name: UserName,
    display_name: UserDisplayName,
    email: EmailAddress,
    password: StringOption<UserPassword>,
    oauth_register_token: StringOption<OAuth2RegistrationToken>,
    recaptcha_response: StringOption<RecaptchaResponse>,
}

async fn create(
    user_service: State<Arc<impl UserFeatureService>>,
    user_agent: UserAgent,
    Json(CreateRequest {
        name,
        display_name,
        email,
        password,
        oauth_register_token,
        recaptcha_response,
    }): Json<CreateRequest>,
) -> Response {
    match user_service
        .create_user(
            UserCreateRequest {
                name,
                display_name,
                email,
                password: password.into(),
                oauth2_registration_token: oauth_register_token.into(),
            },
            user_agent.0.map(DeviceName::from_string_truncated),
            recaptcha_response.into(),
        )
        .await
    {
        Ok(result) => Json(ApiLogin::from(result)).into_response(),
        Err(UserCreateError::NameConflict) => UserAlreadyExistsError.into_response(),
        Err(UserCreateError::EmailConflict) => EmailAlreadyExistsError.into_response(),
        Err(UserCreateError::Recaptcha) => RecaptchaFailedError.into_response(),
        Err(UserCreateError::NoLoginMethod) => NoLoginMethodError.into_response(),
        Err(UserCreateError::InvalidOAuthRegistrationToken) => {
            InvalidOAuthTokenError.into_response()
        }
        Err(UserCreateError::RemoteAlreadyLinked) => RemoteAlreadyLinkedError.into_response(),
        Err(UserCreateError::Other(err)) => internal_server_error(err),
    }
}

fn create_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create a new user account.")
        .description("Also creates a session for the new user.")
        .add_response::<ApiLogin>(StatusCode::OK, None)
        .add_error::<UserAlreadyExistsError>()
        .add_error::<EmailAlreadyExistsError>()
        .add_error::<RecaptchaFailedError>()
        .add_error::<NoLoginMethodError>()
        .add_error::<InvalidOAuthTokenError>()
        .add_error::<RemoteAlreadyLinkedError>()
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct UpdateRequest {
    name: StringOption<UserName>,
    display_name: StringOption<UserDisplayName>,
    email: StringOption<EmailAddress>,
    email_verified: Option<bool>,
    password: Option<ApiUserPasswordOrEmpty>,
    enabled: Option<bool>,
    admin: Option<bool>,
    description: StringOption<UserBio>,
    tags: Option<UserTags>,
    newsletter: Option<bool>,
    business: Option<bool>,
    first_name: StringOption<UserFirstName>,
    last_name: StringOption<UserLastName>,
    street: StringOption<UserStreet>,
    zip_code: StringOption<UserZipCode>,
    city: StringOption<UserCity>,
    country: StringOption<UserCountry>,
    vat_id: StringOption<UserVatId>,
}

async fn update(
    user_service: State<Arc<impl UserFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
    Json(UpdateRequest {
        name,
        display_name,
        email,
        email_verified,
        password,
        enabled,
        admin,
        description,
        tags,
        newsletter,
        business,
        first_name,
        last_name,
        street,
        zip_code,
        city,
        country,
        vat_id,
    }): Json<UpdateRequest>,
) -> Response {
    match user_service
        .update_user(
            &token.0,
            user_id.into(),
            UserUpdateRequest {
                user: UserUpdateUserRequest {
                    name: Option::from(name).into(),
                    email: Option::from(email).into(),
                    email_verified: email_verified.into(),
                    password: password
                        .map(|pw| match pw {
                            ApiUserPasswordOrEmpty::Empty => PasswordUpdate::Remove,
                            ApiUserPasswordOrEmpty::Password(pw) => PasswordUpdate::Change(pw),
                        })
                        .into(),
                    enabled: enabled.into(),
                    admin: admin.into(),
                    newsletter: newsletter.into(),
                },
                profile: UserProfilePatch {
                    display_name: Option::from(display_name).into(),
                    bio: Option::from(description).into(),
                    tags: tags.into(),
                },
                invoice_info: UserInvoiceInfo {
                    business,
                    first_name: first_name.into(),
                    last_name: last_name.into(),
                    street: street.into(),
                    zip_code: zip_code.into(),
                    city: city.into(),
                    country: country.into(),
                    vat_id: vat_id.into(),
                },
            },
        )
        .await
    {
        Ok(user) => Json(ApiUser::from(user)).into_response(),
        Err(UserUpdateError::NotFound) => UserNotFoundError.into_response(),
        Err(UserUpdateError::NameConflict) => UserAlreadyExistsError.into_response(),
        Err(UserUpdateError::EmailConflict) => EmailAlreadyExistsError.into_response(),
        Err(UserUpdateError::CannotRemovePassword) => {
            CannotDeleteLastLoginMethodError.into_response()
        }
        Err(
            UserUpdateError::CannotDisableSelf
            | UserUpdateError::CannotDemoteSelf
            | UserUpdateError::NameChangeRateLimit { .. },
        ) => PermissionDeniedError.into_response(),
        Err(UserUpdateError::NoEmail) => NoEmailError.into_response(),
        Err(UserUpdateError::InvalidVatId) => InvalidVatIdError.into_response(),
        Err(UserUpdateError::Auth(err)) => auth_error(err),
        Err(UserUpdateError::Other(err)) => internal_server_error(err),
    }
}

fn update_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Update the given user.")
        .add_response::<ApiUser>(StatusCode::OK, "The user has been updated.")
        .add_error::<UserNotFoundError>()
        .add_error::<UserAlreadyExistsError>()
        .add_error::<EmailAlreadyExistsError>()
        .add_error::<CannotDeleteLastLoginMethodError>()
        .add_error::<PermissionDeniedError>()
        .add_error::<NoEmailError>()
        .add_error::<InvalidVatIdError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

async fn delete(
    user_service: State<Arc<impl UserFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
) -> Response {
    match user_service.delete_user(&token.0, user_id.into()).await {
        Ok(()) => Json(OkResponse).into_response(),
        Err(UserDeleteError::NotFound) => UserNotFoundError.into_response(),
        Err(UserDeleteError::Auth(err)) => auth_error(err),
        Err(UserDeleteError::Other(err)) => internal_server_error(err),
    }
}

fn delete_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Delete the given user.")
        .add_response::<OkResponse>(StatusCode::OK, "The user has been deleted.")
        .add_error::<UserNotFoundError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

async fn request_verification_email(
    service: State<Arc<impl UserFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
) -> Response {
    match service
        .request_verification_email(&token.0, user_id.into())
        .await
    {
        Ok(()) => Json(OkResponse).into_response(),
        Err(UserRequestVerificationEmailError::NotFound) => UserNotFoundError.into_response(),
        Err(UserRequestVerificationEmailError::AlreadyVerified) => {
            EmailAlreadyVerifiedError.into_response()
        }
        Err(UserRequestVerificationEmailError::NoEmail) => InvalidEmailError.into_response(),
        Err(UserRequestVerificationEmailError::Auth(err)) => auth_error(err),
        Err(UserRequestVerificationEmailError::Other(err)) => internal_server_error(err),
    }
}

fn request_verification_email_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Request a verification email for the given user.")
        .add_response::<OkResponse>(
            StatusCode::OK,
            "The user has been sent a verification email.",
        )
        .add_error::<UserNotFoundError>()
        .add_error::<EmailAlreadyVerifiedError>()
        .add_error::<NoEmailError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct VerifyEmailRequest {
    code: VerificationCode,
}

async fn verify_email(
    service: State<Arc<impl UserFeatureService>>,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
    Json(VerifyEmailRequest { code }): Json<VerifyEmailRequest>,
) -> Response {
    let ApiUserIdOrSelf::Slf = user_id else {
        return CanOnlyVerifyEmailForSelfError.into_response();
    };

    match service.verify_email(code).await {
        Ok(()) => Json(OkResponse).into_response(),
        Err(UserVerifyEmailError::InvalidCode) => InvalidVerificationCodeError.into_response(),
        Err(UserVerifyEmailError::Other(err)) => internal_server_error(err),
    }
}

fn verify_email_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Verify a user's email address using a verification code.")
        .add_response::<OkResponse>(
            StatusCode::OK,
            "The user's email address has been verified.",
        )
        .add_error::<InvalidVerificationCodeError>()
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct VerifyNewsletterSubscriptionRequest {
    code: VerificationCode,
}

async fn verify_newsletter_subscription(
    service: State<Arc<impl UserFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
    Json(VerifyNewsletterSubscriptionRequest { code }): Json<VerifyNewsletterSubscriptionRequest>,
) -> Response {
    match service
        .verify_newsletter_subscription(&token.0, user_id.into(), code)
        .await
    {
        Ok(user) => Json(ApiUser::from(user)).into_response(),
        Err(UserVerifyNewsletterSubscriptionError::NotFound) => UserNotFoundError.into_response(),
        Err(UserVerifyNewsletterSubscriptionError::AlreadySubscribed) => {
            NewsletterAlreadySubscribedError.into_response()
        }
        Err(UserVerifyNewsletterSubscriptionError::InvalidCode) => {
            InvalidVerificationCodeError.into_response()
        }
        Err(UserVerifyNewsletterSubscriptionError::Auth(err)) => auth_error(err),
        Err(UserVerifyNewsletterSubscriptionError::Other(err)) => internal_server_error(err),
    }
}

fn verify_newsletter_subscription_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Verify the newsletter subscription using a verification code.")
        .add_response::<ApiUser>(
            StatusCode::OK,
            "The user's newsletter subscription has been verified.",
        )
        .add_error::<UserNotFoundError>()
        .add_error::<NewsletterAlreadySubscribedError>()
        .add_error::<InvalidVerificationCodeError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct RequestPasswordResetRequest {
    email: EmailAddress,
    recaptcha_response: StringOption<RecaptchaResponse>,
}

async fn request_password_reset(
    service: State<Arc<impl UserFeatureService>>,
    Json(RequestPasswordResetRequest {
        email,
        recaptcha_response,
    }): Json<RequestPasswordResetRequest>,
) -> Response {
    match service
        .request_password_reset(email, recaptcha_response.into())
        .await
    {
        Ok(()) => Json(OkResponse).into_response(),
        Err(UserRequestPasswordResetError::Recaptcha) => RecaptchaFailedError.into_response(),
        Err(UserRequestPasswordResetError::Other(err)) => internal_server_error(err),
    }
}

fn request_password_reset_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Request a password reset email.")
        .add_response::<OkResponse>(
            StatusCode::OK,
            "The user has been sent a password reset email.",
        )
        .add_error::<RecaptchaFailedError>()
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct ResetPasswordRequest {
    email: EmailAddress,
    code: VerificationCode,
    password: UserPassword,
}

async fn reset_password(
    service: State<Arc<impl UserFeatureService>>,
    Json(ResetPasswordRequest {
        email,
        code,
        password,
    }): Json<ResetPasswordRequest>,
) -> Response {
    match service.reset_password(email, code, password).await {
        Ok(user) => Json(ApiUser::from(user)).into_response(),
        Err(UserResetPasswordError::Failed) => PasswordResetFailedError.into_response(),
        Err(UserResetPasswordError::Other(err)) => internal_server_error(err),
    }
}

fn reset_password_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Reset a user's password using a password reset verification code.")
        .add_response::<ApiUser>(StatusCode::OK, "The user's password has been changed.")
        .add_error::<PasswordResetFailedError>()
        .with(internal_server_error_docs)
}

error_code! {
    /// The user does not exist.
    pub UserNotFoundError(NOT_FOUND, "User not found");
    /// The user account has been disabled.
    pub UserDisabledError(FORBIDDEN, "User disabled");
    /// The last login method (password or OAuth2 link) cannot be deleted.
    pub CannotDeleteLastLoginMethodError(FORBIDDEN, "Cannot delete last login method");
    /// A user with this name already exists.
    UserAlreadyExistsError(CONFLICT, "User already exists");
    /// A user with this email address already exists.
    EmailAlreadyExistsError(CONFLICT, "Email already exists");
    /// No login method was provided.
    NoLoginMethodError(PRECONDITION_FAILED, "No login method");
    /// The OAuth2 registration token is invalid or has expired.
    InvalidOAuthTokenError(UNAUTHORIZED, "Invalid OAuth token");
    /// The vat id is invalid.
    InvalidVatIdError(NOT_FOUND, "Invalid VAT ID");
    /// The email or password reset code is invalid or the reset code has expired.
    PasswordResetFailedError(UNAUTHORIZED, "Password reset failed");
    /// The verification code is invalid.
    InvalidVerificationCodeError(UNAUTHORIZED, "Invalid verification code");
    /// The user is already subscribed to the newsletter.
    NewsletterAlreadySubscribedError(CONFLICT, "Newsletter already subscribed");
    /// The user does not have an email address.
    NoEmailError(FORBIDDEN, "No email");
    /// The user's email address has already been verified.
    EmailAlreadyVerifiedError(PRECONDITION_FAILED, "Email already verified");
    /// The email address is invalid.
    InvalidEmailError(BAD_REQUEST, "Invalid email");
    /// Only the email address of the currently authenticated user can be verified.
    CanOnlyVerifyEmailForSelfError(BAD_REQUEST, "Can only verify email for self");
}
