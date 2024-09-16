use academy_auth_contracts::MockAuthService;
use academy_core_mfa_contracts::authenticate::MockMfaAuthenticateService;
use academy_core_session_contracts::{
    failed_auth_count::MockSessionFailedAuthCountService, session::MockSessionService,
};
use academy_core_user_contracts::queries::get_by_name_or_email::MockUserGetByNameOrEmailQueryService;
use academy_persistence_contracts::{
    session::MockSessionRepository, user::MockUserRepository, MockDatabase, MockTransaction,
};
use academy_shared_contracts::captcha::MockCaptchaService;

use crate::{SessionFeatureServiceImpl, SessionServiceConfig};

mod create_session;
mod delete_by_user;
mod delete_current_session;
mod delete_session;
mod get_current_session;
mod impersonate;
mod list_by_user;
mod refresh;

type Sut = SessionFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockCaptchaService,
    MockSessionService<MockTransaction>,
    MockSessionFailedAuthCountService,
    MockUserGetByNameOrEmailQueryService<MockTransaction>,
    MockMfaAuthenticateService<MockTransaction>,
    MockUserRepository<MockTransaction>,
    MockSessionRepository<MockTransaction>,
>;

impl Default for SessionServiceConfig {
    fn default() -> Self {
        Self {
            login_fails_before_captcha: 3,
        }
    }
}
