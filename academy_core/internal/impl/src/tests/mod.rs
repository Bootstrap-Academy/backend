use academy_auth_contracts::internal::MockAuthInternalService;
use academy_core_coin_contracts::coin::MockCoinService;
use academy_persistence_contracts::{user::MockUserRepository, MockDatabase, MockTransaction};

use crate::InternalServiceImpl;

mod add_coins;
mod get_user;
mod get_user_by_email;

type Sut = InternalServiceImpl<
    MockDatabase,
    MockAuthInternalService,
    MockUserRepository<MockTransaction>,
    MockCoinService<MockTransaction>,
>;
