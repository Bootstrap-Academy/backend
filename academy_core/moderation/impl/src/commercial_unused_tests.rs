//! Test-only unused dependencies: every call fails. No production mock changes.

#![allow(unused_imports, unused_variables)]

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
        fn prove_recipient(
            &self,
            client_ip: IpAddr,
            cmd: SessionCreateCommand,
            recaptcha_response: Option<RecaptchaResponse>,
        ) -> impl Future<Output = Result<UserId, SessionCreateError>> + Send {
            async { panic!("unused feature called") }
        }
        fn get_current_session(
            &self,
            token: &AccessToken,
        ) -> impl Future<Output = Result<Session, SessionGetCurrentError>> + Send {
            async { panic!("unused feature called") }
        }
        fn list_by_user(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> impl Future<Output = Result<Vec<Session>, SessionListByUserError>> + Send {
            async { panic!("unused feature called") }
        }
        fn create_session(
            &self,
            client_ip: IpAddr,
            cmd: SessionCreateCommand,
            recaptcha_response: Option<RecaptchaResponse>,
        ) -> impl Future<Output = Result<Login, SessionCreateError>> + Send {
            async { panic!("unused feature called") }
        }
        fn impersonate(
            &self,
            token: &AccessToken,
            user_id: UserId,
        ) -> impl Future<Output = Result<Login, SessionImpersonateError>> + Send {
            async { panic!("unused feature called") }
        }
        fn refresh_session(
            &self,
            refresh_token: &RefreshToken,
        ) -> impl Future<Output = Result<Login, SessionRefreshError>> + Send {
            async { panic!("unused feature called") }
        }
        fn delete_session(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
            session_id: SessionId,
        ) -> impl Future<Output = Result<(), SessionDeleteError>> + Send {
            async { panic!("unused feature called") }
        }
        fn delete_current_session(
            &self,
            token: &AccessToken,
        ) -> impl Future<Output = Result<(), SessionDeleteCurrentError>> + Send {
            async { panic!("unused feature called") }
        }
        fn delete_by_user(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> impl Future<Output = Result<(), SessionDeleteByUserError>> + Send {
            async { panic!("unused feature called") }
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
        fn begin_recipient(
            &self,
            provider: OAuth2ProviderId,
            redirect: Url,
        ) -> impl Future<Output = anyhow::Result<OAuth2AuthorizationUrl>> + Send {
            async { panic!("unused feature called") }
        }
        fn prove_recipient(
            &self,
            callback: OAuth2Callback,
        ) -> impl Future<Output = anyhow::Result<UserId>> + Send {
            async { panic!("unused feature called") }
        }
        fn list_providers(&self) -> Vec<OAuth2ProviderSummary> {
            panic!("unused feature called")
        }
        fn begin_authorization(
            &self,
            token: &AccessToken,
            provider_id: OAuth2ProviderId,
            redirect_uri: Url,
        ) -> impl Future<Output = Result<OAuth2AuthorizationUrl, OAuth2BeginAuthorizationError>> + Send
        {
            async { panic!("unused feature called") }
        }
        fn list_links(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> impl Future<Output = Result<Vec<OAuth2Link>, OAuth2ListLinksError>> + Send {
            async { panic!("unused feature called") }
        }
        fn create_link(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
            callback: OAuth2Callback,
        ) -> impl Future<Output = Result<OAuth2Link, OAuth2CreateLinkError>> + Send {
            async { panic!("unused feature called") }
        }
        fn delete_link(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
            link_id: OAuth2LinkId,
        ) -> impl Future<Output = Result<(), OAuth2DeleteLinkError>> + Send {
            async { panic!("unused feature called") }
        }
        fn create_session(
            &self,
            callback: OAuth2Callback,
            device_name: Option<DeviceName>,
        ) -> impl Future<Output = Result<OAuth2CreateSessionResponse, OAuth2CreateSessionError>> + Send
        {
            async { panic!("unused feature called") }
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
        fn list_users(
            &self,
            token: &AccessToken,
            query: UserListQuery,
        ) -> impl Future<Output = Result<UserListResult, UserListError>> + Send {
            async { panic!("unused feature called") }
        }
        fn get_user(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> impl Future<Output = Result<UserComposite, UserGetError>> + Send {
            async { panic!("unused feature called") }
        }
        fn create_user(
            &self,
            request: UserCreateRequest,
            device_name: Option<DeviceName>,
            recaptcha_response: Option<RecaptchaResponse>,
        ) -> impl Future<Output = Result<Login, UserCreateError>> + Send {
            async { panic!("unused feature called") }
        }
        fn update_user(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
            request: UserUpdateRequest,
        ) -> impl Future<Output = Result<UserComposite, UserUpdateError>> + Send {
            async { panic!("unused feature called") }
        }
        fn accept_terms(
            &self,
            token: &AccessToken,
            request: UserAcceptTermsRequest,
        ) -> impl Future<Output = Result<UserComposite, UserAcceptTermsError>> + Send {
            async { panic!("unused feature called") }
        }
        fn decline_terms(
            &self,
            token: &AccessToken,
        ) -> impl Future<Output = Result<UserComposite, UserDeclineTermsError>> + Send {
            async { panic!("unused feature called") }
        }
        fn delete_user(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> impl Future<Output = Result<(), UserDeleteError>> + Send {
            async { panic!("unused feature called") }
        }
        fn recipient_delete(
            &self,
            user_id: academy_models::user::UserId,
        ) -> impl Future<Output = Result<(), UserDeleteError>> + Send {
            async { panic!("unused feature called") }
        }
        fn export_user_data(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> impl Future<Output = Result<UserDataExport, UserExportError>> + Send {
            async { panic!("unused feature called") }
        }
        fn request_verification_email(
            &self,
            token: &AccessToken,
            user_id: UserIdOrSelf,
        ) -> impl Future<Output = Result<(), UserRequestVerificationEmailError>> + Send {
            async { panic!("unused feature called") }
        }
        fn verify_email(
            &self,
            code: VerificationCode,
        ) -> impl Future<Output = Result<(), UserVerifyEmailError>> + Send {
            async { panic!("unused feature called") }
        }
        fn request_password_reset(
            &self,
            email: EmailAddress,
            recaptcha_response: Option<RecaptchaResponse>,
        ) -> impl Future<Output = Result<(), UserRequestPasswordResetError>> + Send {
            async { panic!("unused feature called") }
        }
        fn reset_password(
            &self,
            email: EmailAddress,
            code: VerificationCode,
            new_password: UserPassword,
        ) -> impl Future<Output = Result<UserComposite, UserResetPasswordError>> + Send {
            async { panic!("unused feature called") }
        }
    }
}
