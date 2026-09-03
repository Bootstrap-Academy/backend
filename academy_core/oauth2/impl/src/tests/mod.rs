use std::{collections::HashMap, time::Duration};

use academy_auth_contracts::MockAuthService;
use academy_core_oauth2_contracts::{
    authorization::MockOAuth2AuthorizationService, link::MockOAuth2LinkService,
    login::MockOAuth2LoginService, registration::MockOAuth2RegistrationService,
};
use academy_core_session_contracts::session::MockSessionService;
use academy_demo::oauth2::{TEST_OAUTH2_PROVIDER, TEST_OAUTH2_PROVIDER_ID};
use academy_models::oauth2::{OAuth2Callback, OAuth2Login, OAuth2PendingAuthorization};
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, oauth2::MockOAuth2Repository, user::MockUserRepository,
};

use crate::{OAuth2FeatureConfig, OAuth2FeatureServiceImpl};

mod begin_authorization;
mod create_link;
mod create_session;
mod delete_link;
mod list_links;
mod list_providers;

type Sut = OAuth2FeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockUserRepository<MockTransaction>,
    MockOAuth2Repository<MockTransaction>,
    MockOAuth2LinkService<MockTransaction>,
    MockOAuth2AuthorizationService,
    MockOAuth2LoginService,
    MockOAuth2RegistrationService,
    MockSessionService<MockTransaction>,
>;

const STATE: &str = "vJyLhIytPtnPKfJVpTPdRVQdEuYSbcCwOTrnegrmwtIkVfSHiWMSuxaMYrgvhbTx";
const CODE_VERIFIER: &str = "zBHUHfHnQEqmMEHZjbWpMvRfEZLzCcuHVowoWaFsyoBUmzcvcgcAKPqolqZcBEIN";

/// The callback a client submits after the provider redirected it back.
fn callback() -> OAuth2Callback {
    OAuth2Callback {
        state: STATE.try_into().unwrap(),
        code: "code".try_into().unwrap(),
    }
}

/// The authorization the [`callback`] redeems.
fn pending_authorization() -> OAuth2PendingAuthorization {
    OAuth2PendingAuthorization {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
        code_verifier: Some(CODE_VERIFIER.try_into().unwrap()),
    }
}

/// The login the feature service derives from [`callback`] and
/// [`pending_authorization`].
fn login() -> OAuth2Login {
    OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
        code_verifier: Some(CODE_VERIFIER.try_into().unwrap()),
    }
}

impl Default for OAuth2FeatureConfig {
    fn default() -> Self {
        Self {
            registration_token_ttl: Duration::from_secs(600),
            authorization_ttl: Duration::from_secs(600),
            providers: HashMap::from([(
                TEST_OAUTH2_PROVIDER_ID.clone(),
                TEST_OAUTH2_PROVIDER.clone(),
            )])
            .into(),
        }
    }
}
