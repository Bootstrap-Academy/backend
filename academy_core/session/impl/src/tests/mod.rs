use academy_auth_contracts::MockAuthService;
use academy_core_mfa_contracts::authenticate::MockMfaAuthenticateService;
use academy_core_session_contracts::{
    failed_auth_count::MockSessionFailedAuthCountService, session::MockSessionService,
};
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, session::MockSessionRepository, user::MockUserRepository,
};
use academy_shared_contracts::captcha::MockCaptchaService;

use crate::{SessionFeatureConfig, SessionFeatureServiceImpl};

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
    MockMfaAuthenticateService<MockTransaction>,
    MockUserRepository<MockTransaction>,
    MockSessionRepository<MockTransaction>,
>;

impl Default for SessionFeatureConfig {
    fn default() -> Self {
        Self {
            login_fails_before_captcha: 3,
        }
    }
}
