use std::time::Duration;

use academy_auth_contracts::MockAuthService;
use academy_core_oauth2_contracts::registration::MockOAuth2RegistrationService;
use academy_core_session_contracts::session::MockSessionService;
use academy_core_user_contracts::{
    email_confirmation::MockUserEmailConfirmationService, update::MockUserUpdateService,
    user::MockUserService,
};
use academy_extern_contracts::vat::MockVatApiService;
use academy_persistence_contracts::{
    coin::MockCoinRepository, user::MockUserRepository, MockDatabase, MockTransaction,
};
use academy_shared_contracts::captcha::MockCaptchaService;

use crate::{UserFeatureConfig, UserFeatureServiceImpl};

mod create_user;
mod delete_user;
mod get_user;
mod list_users;
mod request_password_reset;
mod request_verification_email;
mod reset_password;
mod update_user;
mod verify_email;
mod verify_newsletter_subscription;

type Sut = UserFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockCaptchaService,
    MockVatApiService,
    MockUserService<MockTransaction>,
    MockUserEmailConfirmationService<MockTransaction>,
    MockUserUpdateService<MockTransaction>,
    MockSessionService<MockTransaction>,
    MockOAuth2RegistrationService,
    MockUserRepository<MockTransaction>,
    MockCoinRepository<MockTransaction>,
>;

impl Default for UserFeatureConfig {
    fn default() -> Self {
        Self {
            name_change_rate_limit: Duration::from_secs(30 * 24 * 3600),
            verification_redirect_url: "https://bootstrap.academy/auth/verify-account"
                .to_owned()
                .into(),
            verification_verification_code_ttl: Duration::from_secs(3600),
            password_reset_redirect_url: "https://bootstrap.academy/auth/reset-password"
                .to_owned()
                .into(),
            password_reset_verification_code_ttl: Duration::from_secs(3600),
            newsletter_subscription_redirect_url: "https://bootstrap.academy/account/newsletter"
                .to_owned()
                .into(),
            newsletter_subscription_verification_code_ttl: Duration::from_secs(3600),
        }
    }
}
