use academy_auth_contracts::MockAuthService;
use academy_core_coin_contracts::coin::MockCoinService;
use academy_core_finance_contracts::coin::MockFinanceCoinService;
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, coin::MockCoinRepository, user::MockUserRepository,
};

use crate::CoinFeatureServiceImpl;

mod add_coins;
mod get_balance;
mod get_config;

type Sut = CoinFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockUserRepository<MockTransaction>,
    MockCoinRepository<MockTransaction>,
    MockCoinService<MockTransaction>,
    MockFinanceCoinService,
>;
