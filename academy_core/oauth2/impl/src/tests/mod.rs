use std::{collections::HashMap, time::Duration};

use academy_auth_contracts::MockAuthService;
use academy_cache_contracts::MockCacheService;
use academy_core_oauth2_contracts::{
    create_link::MockOAuth2CreateLinkService, login::MockOAuth2LoginService,
};
use academy_core_session_contracts::session::MockSessionService;
use academy_demo::oauth2::{TEST_OAUTH2_PROVIDER, TEST_OAUTH2_PROVIDER_ID};
use academy_extern_contracts::oauth2::MockOAuth2ApiService;
use academy_persistence_contracts::{
    oauth2::MockOAuth2Repository, user::MockUserRepository, MockDatabase, MockTransaction,
};
use academy_shared_contracts::secret::MockSecretService;

use crate::{OAuth2FeatureServiceImpl, OAuth2ServiceConfig};

mod create_link;
mod create_session;
mod delete_link;
mod list_links;
mod list_providers;

type Sut = OAuth2FeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockCacheService,
    MockSecretService,
    MockOAuth2ApiService,
    MockUserRepository<MockTransaction>,
    MockOAuth2Repository<MockTransaction>,
    MockOAuth2CreateLinkService<MockTransaction>,
    MockOAuth2LoginService,
    MockSessionService<MockTransaction>,
>;

impl Default for OAuth2ServiceConfig {
    fn default() -> Self {
        Self {
            registration_token_ttl: Duration::from_secs(600),
            providers: HashMap::from([(
                TEST_OAUTH2_PROVIDER_ID.clone(),
                TEST_OAUTH2_PROVIDER.clone(),
            )])
            .into(),
        }
    }
}
