//! Test-only unused dependencies: every call fails. No production mock changes.

#![allow(
    unused_imports,
    unused_variables,
    reason = "Fail-on-call test dependencies retain trait parameter types without using their values"
)]

#[derive(Default)]
pub struct Unused;

mod session {
    use super::Unused;
    use academy_core_session_contracts::*;
    use academy_models::{
        RecaptchaResponse,
        auth::{AccessToken, AuthError, Login, RefreshToken},
        mfa::MfaAuthentication,
        session::{DeviceName, Session, SessionId},
        user::{UserId, UserIdOrSelf, UserNameOrEmailAddress, UserPassword},
    };
    use std::{future::Future, net::IpAddr, time::Duration};
    impl SessionFeatureService for Unused {
        async fn prove_recipient(
            &self,
            client_ip: IpAddr,
            cmd: SessionCreateCommand,
            recaptcha_response: Option<RecaptchaResponse>,
        ) -> Result<UserId, SessionCreateError> {
            panic!("unused feature called")
        }
        async fn get_current_session(
            &self,
            token: &AccessToken,
        ) -> Result<Session, SessionGetCurrentError> {
            panic!("unused feature called")
        }
        async fn list_by_user(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> Result<Vec<Session>, SessionListByUserError> {
            panic!("unused feature called")
        }
        async fn create_session(
            &self,
            client_ip: IpAddr,
            cmd: SessionCreateCommand,
            recaptcha_response: Option<RecaptchaResponse>,
        ) -> Result<Login, SessionCreateError> {
            panic!("unused feature called")
        }
        async fn impersonate(
            &self,
            token: &AccessToken,
            user_id: UserId,
        ) -> Result<Login, SessionImpersonateError> {
            panic!("unused feature called")
        }
        async fn refresh_session(
            &self,
            refresh_token: &RefreshToken,
        ) -> Result<Login, SessionRefreshError> {
            panic!("unused feature called")
        }
        async fn delete_session(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
            session_id: SessionId,
        ) -> Result<(), SessionDeleteError> {
            panic!("unused feature called")
        }
        async fn delete_current_session(
            &self,
            token: &AccessToken,
        ) -> Result<(), SessionDeleteCurrentError> {
            panic!("unused feature called")
        }
        async fn delete_by_user(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> Result<(), SessionDeleteByUserError> {
            panic!("unused feature called")
        }
    }
}

mod oauth2 {
    use super::Unused;
    use academy_core_oauth2_contracts::*;
    use academy_models::{
        auth::{AccessToken, AuthError, Login},
        oauth2::{
            OAuth2AuthorizationUrl, OAuth2Callback, OAuth2Link, OAuth2LinkId, OAuth2ProviderId,
            OAuth2ProviderSummary, OAuth2RegistrationToken,
        },
        session::DeviceName,
        url::Url,
        user::{UserId, UserIdOrSelf},
    };
    use std::future::Future;
    impl OAuth2FeatureService for Unused {
        async fn begin_recipient(
            &self,
            provider: OAuth2ProviderId,
            redirect: Url,
        ) -> anyhow::Result<OAuth2AuthorizationUrl> {
            panic!("unused feature called")
        }
        async fn prove_recipient(&self, callback: OAuth2Callback) -> anyhow::Result<UserId> {
            panic!("unused feature called")
        }
        fn list_providers(&self) -> Vec<OAuth2ProviderSummary> {
            panic!("unused feature called")
        }
        async fn begin_authorization(
            &self,
            token: &AccessToken,
            provider_id: OAuth2ProviderId,
            redirect_uri: Url,
        ) -> Result<OAuth2AuthorizationUrl, OAuth2BeginAuthorizationError> {
            panic!("unused feature called")
        }
        async fn list_links(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> Result<Vec<OAuth2Link>, OAuth2ListLinksError> {
            panic!("unused feature called")
        }
        async fn create_link(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
            callback: OAuth2Callback,
        ) -> Result<OAuth2Link, OAuth2CreateLinkError> {
            panic!("unused feature called")
        }
        async fn delete_link(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
            link_id: OAuth2LinkId,
        ) -> Result<(), OAuth2DeleteLinkError> {
            panic!("unused feature called")
        }
        async fn create_session(
            &self,
            callback: OAuth2Callback,
            device_name: Option<DeviceName>,
        ) -> Result<OAuth2CreateSessionResponse, OAuth2CreateSessionError> {
            panic!("unused feature called")
        }
    }
}

mod user {
    use super::Unused;
    use academy_core_user_contracts::*;
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
    use chrono::{DateTime, Utc};
    use export::UserDataExport;
    use std::future::Future;
    use user::{UserListQuery, UserListResult};
    impl UserFeatureService for Unused {
        async fn list_users(
            &self,
            token: &AccessToken,
            query: UserListQuery,
        ) -> Result<UserListResult, UserListError> {
            panic!("unused feature called")
        }
        async fn get_user(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> Result<UserComposite, UserGetError> {
            panic!("unused feature called")
        }
        async fn create_user(
            &self,
            request: UserCreateRequest,
            device_name: Option<DeviceName>,
            recaptcha_response: Option<RecaptchaResponse>,
        ) -> Result<Login, UserCreateError> {
            panic!("unused feature called")
        }
        async fn update_user(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
            request: UserUpdateRequest,
        ) -> Result<UserComposite, UserUpdateError> {
            panic!("unused feature called")
        }
        async fn accept_terms(
            &self,
            token: &AccessToken,
            request: UserAcceptTermsRequest,
        ) -> Result<UserComposite, UserAcceptTermsError> {
            panic!("unused feature called")
        }
        async fn decline_terms(
            &self,
            token: &AccessToken,
        ) -> Result<UserComposite, UserDeclineTermsError> {
            panic!("unused feature called")
        }
        async fn delete_user(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> Result<(), UserDeleteError> {
            panic!("unused feature called")
        }
        async fn recipient_delete(
            &self,
            user_id: academy_models::user::UserId,
        ) -> Result<(), UserDeleteError> {
            panic!("unused feature called")
        }
        async fn export_user_data(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> Result<UserDataExport, UserExportError> {
            panic!("unused feature called")
        }
        async fn request_verification_email(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> Result<(), UserRequestVerificationEmailError> {
            panic!("unused feature called")
        }
        async fn verify_email(&self, code: VerificationCode) -> Result<(), UserVerifyEmailError> {
            panic!("unused feature called")
        }
        async fn request_password_reset(
            &self,
            email: EmailAddress,
            recaptcha_response: Option<RecaptchaResponse>,
        ) -> Result<(), UserRequestPasswordResetError> {
            panic!("unused feature called")
        }
        async fn reset_password(
            &self,
            email: EmailAddress,
            code: VerificationCode,
            new_password: UserPassword,
        ) -> Result<UserComposite, UserResetPasswordError> {
            panic!("unused feature called")
        }
    }
}
