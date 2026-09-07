use std::{
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

use academy_auth_contracts::MockAuthService;
use academy_core_mfa_contracts::authenticate::MockMfaAuthenticateService;
use academy_core_session_contracts::{
    failed_auth_count::MockSessionFailedAuthCountService,
    login_throttle::MockSessionLoginThrottleService, session::MockSessionService,
};
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, session::MockSessionRepository, user::MockUserRepository,
};
use academy_shared_contracts::captcha::MockCaptchaService;

use crate::{
    SessionFeatureConfig, SessionFeatureServiceImpl, login_throttle::SessionLoginThrottleConfig,
};

/// Address every login attempt in these tests comes from.
const CLIENT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));

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
    MockSessionLoginThrottleService,
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

impl Default for SessionLoginThrottleConfig {
    fn default() -> Self {
        Self {
            fails_before_lock: 5,
            fail_window: Duration::from_secs(15 * 60),
            lock_initial: Duration::from_secs(60),
            lock_max: Duration::from_secs(15 * 60),
            fails_per_ip: 30,
            ip_window: Duration::from_secs(15 * 60),
        }
    }
}
