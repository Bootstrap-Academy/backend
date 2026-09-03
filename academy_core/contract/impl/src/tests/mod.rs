use std::{
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
    time::Duration,
};

use academy_auth_contracts::MockAuthService;
use academy_cache_contracts::MockCacheService;
use academy_demo::{SHA256HASH1, SHA256HASH1_HEX, SHA256HASH2, SHA256HASH2_HEX, user::FOO};
use academy_email_contracts::{
    ContentType, Email, MockEmailService, template::MockTemplateEmailService,
};
use academy_models::{
    contract::{ContractDeclarantName, ContractDeclaration, ContractDeclarationDetails},
    email_address::EmailAddress,
};
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, contract::MockContractRepository,
    premium::MockPremiumRepository, user::MockUserRepository,
};
use academy_shared_contracts::{hash::MockHashService, id::MockIdService, time::MockTimeService};

use crate::{ContractFeatureConfig, ContractFeatureServiceImpl, internal_notification_body};

mod declare_cancellation;
mod declare_withdrawal;
mod list_declarations;

type Sut = ContractFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockIdService,
    MockTimeService,
    MockCacheService,
    MockHashService,
    MockTemplateEmailService,
    MockEmailService,
    MockUserRepository<MockTransaction>,
    MockPremiumRepository<MockTransaction>,
    MockContractRepository<MockTransaction>,
>;

const CLIENT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 42));
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(3600);
const RATE_LIMIT_COUNT: u64 = 5;

impl Default for ContractFeatureConfig {
    fn default() -> Self {
        Self {
            internal_email: Arc::new("contact@example.com".parse().unwrap()),
            rate_limit_window: RATE_LIMIT_WINDOW,
            rate_limit_count: RATE_LIMIT_COUNT,
        }
    }
}

fn declarant_name() -> ContractDeclarantName {
    "Max Mustermann".try_into().unwrap()
}

fn declarant_email() -> EmailAddress {
    FOO.user.email.clone().unwrap()
}

fn unknown_email() -> EmailAddress {
    "nobody@example.com".parse().unwrap()
}

fn no_details() -> ContractDeclarationDetails {
    "".try_into().unwrap()
}

fn ip_cache_key() -> String {
    format!("contract_declaration_rate_limit_ip_{SHA256HASH1_HEX}")
}

fn email_cache_key() -> String {
    format!("contract_declaration_rate_limit_email_{SHA256HASH2_HEX}")
}

/// Hash service expecting the two rate limit keys to be hashed.
fn make_hash(email: &EmailAddress) -> MockHashService {
    MockHashService::new()
        .with_sha256(CLIENT_IP.to_string(), *SHA256HASH1)
        .with_sha256(email.as_str().to_lowercase(), *SHA256HASH2)
}

/// Cache service expecting both rate limit counters to be read and incremented.
fn make_cache(ip_count: u64, email_count: u64) -> MockCacheService {
    MockCacheService::new()
        .with_get(ip_cache_key(), Some(ip_count))
        .with_get(email_cache_key(), Some(email_count))
        .with_set(ip_cache_key(), ip_count + 1, Some(RATE_LIMIT_WINDOW))
        .with_set(email_cache_key(), email_count + 1, Some(RATE_LIMIT_WINDOW))
}

/// Cache service expecting both rate limit counters to be read only.
fn make_exhausted_cache(ip_count: u64, email_count: u64) -> MockCacheService {
    MockCacheService::new()
        .with_get(ip_cache_key(), Some(ip_count))
        .with_get(email_cache_key(), Some(email_count))
}

fn make_internal_email(declaration: &ContractDeclaration, subject: &str) -> Email {
    Email {
        recipient: "contact@example.com".parse().unwrap(),
        subject: subject.into(),
        body: internal_notification_body(declaration),
        content_type: ContentType::Text,
        reply_to: Some(
            declaration
                .email
                .clone()
                .with_name(declaration.name.clone().into_inner()),
        ),
        attachments: Vec::new(),
    }
}
