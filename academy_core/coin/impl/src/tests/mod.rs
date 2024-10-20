use academy_auth_contracts::MockAuthService;
use academy_persistence_contracts::{
    coin::MockCoinRepository, user::MockUserRepository, MockDatabase, MockTransaction,
};

use crate::CoinFeatureServiceImpl;

mod add_coins;
mod get_balance;

type Sut = CoinFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockUserRepository<MockTransaction>,
    MockCoinRepository<MockTransaction>,
>;
