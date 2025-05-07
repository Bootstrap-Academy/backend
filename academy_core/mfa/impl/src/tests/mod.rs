use academy_auth_contracts::MockAuthService;
use academy_core_mfa_contracts::{
    disable::MockMfaDisableService, recovery::MockMfaRecoveryService,
    totp_device::MockMfaTotpDeviceService,
};
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, mfa::MockMfaRepository, user::MockUserRepository,
};

use crate::MfaFeatureServiceImpl;

mod disable;
mod enable;
mod initialize;

type Sut = MfaFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockUserRepository<MockTransaction>,
    MockMfaRepository<MockTransaction>,
    MockMfaRecoveryService<MockTransaction>,
    MockMfaDisableService<MockTransaction>,
    MockMfaTotpDeviceService<MockTransaction>,
>;
