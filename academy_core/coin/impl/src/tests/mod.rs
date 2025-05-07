use academy_auth_contracts::MockAuthService;
use academy_core_coin_contracts::coin::MockCoinService;
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, coin::MockCoinRepository, user::MockUserRepository,
};

use crate::CoinFeatureServiceImpl;

mod add_coins;
mod get_balance;

type Sut = CoinFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockUserRepository<MockTransaction>,
    MockCoinRepository<MockTransaction>,
    MockCoinService<MockTransaction>,
>;
