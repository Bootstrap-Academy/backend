use std::time::Duration;

use academy_auth_contracts::{
    access_token::MockAuthAccessTokenService, refresh_token::MockAuthRefreshTokenService,
};
use academy_persistence_contracts::{session::MockSessionRepository, user::MockUserRepository};
use academy_shared_contracts::{password::MockPasswordService, time::MockTimeService};

use crate::{AuthServiceConfig, AuthServiceImpl};

mod authenticate;
mod authenticate_by_password;
mod authenticate_by_refresh_token;
mod invalidate_access_tokens;
mod issue_tokens;

type Sut = AuthServiceImpl<
    MockTimeService,
    MockPasswordService,
    MockUserRepository<academy_persistence_contracts::MockTransaction>,
    MockSessionRepository<academy_persistence_contracts::MockTransaction>,
    MockAuthAccessTokenService,
    MockAuthRefreshTokenService,
    academy_persistence_contracts::MockDatabase,
>;

impl Default for AuthServiceConfig {
    fn default() -> Self {
        Self {
            access_token_ttl: Duration::from_secs(120),
            refresh_token_ttl: Duration::from_secs(30 * 24 * 3600),
            refresh_token_length: 64,
            internal_token_ttl: Duration::from_secs(10),
        }
    }
}
