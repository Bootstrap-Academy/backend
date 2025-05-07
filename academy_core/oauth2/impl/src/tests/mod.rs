use std::{collections::HashMap, time::Duration};

use academy_auth_contracts::MockAuthService;
use academy_core_oauth2_contracts::{
    link::MockOAuth2LinkService, login::MockOAuth2LoginService,
    registration::MockOAuth2RegistrationService,
};
use academy_core_session_contracts::session::MockSessionService;
use academy_demo::oauth2::{TEST_OAUTH2_PROVIDER, TEST_OAUTH2_PROVIDER_ID};
use academy_extern_contracts::oauth2::MockOAuth2ApiService;
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, oauth2::MockOAuth2Repository, user::MockUserRepository,
};

use crate::{OAuth2FeatureConfig, OAuth2FeatureServiceImpl};

mod create_link;
mod create_session;
mod delete_link;
mod list_links;
mod list_providers;

type Sut = OAuth2FeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockOAuth2ApiService,
    MockUserRepository<MockTransaction>,
    MockOAuth2Repository<MockTransaction>,
    MockOAuth2LinkService<MockTransaction>,
    MockOAuth2LoginService,
    MockOAuth2RegistrationService,
    MockSessionService<MockTransaction>,
>;

impl Default for OAuth2FeatureConfig {
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
