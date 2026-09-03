use academy_auth_contracts::MockAuthService;
use academy_core_withdrawal_contracts::consent::MockWithdrawalConsentService;
use academy_persistence_contracts::{MockDatabase, MockTransaction};

use crate::WithdrawalFeatureServiceImpl;

mod record_consent;

type Sut = WithdrawalFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockWithdrawalConsentService<MockTransaction>,
>;
