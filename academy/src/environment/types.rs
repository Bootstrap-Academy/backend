//! Type aliases for production implementations of all service traits

use academy_auth_impl::{
    AuthServiceImpl, access_token::AuthAccessTokenServiceImpl, internal::AuthInternalServiceImpl,
    refresh_token::AuthRefreshTokenServiceImpl,
};
use academy_cache_valkey::ValkeyCache;
use academy_core_coin_impl::{CoinFeatureServiceImpl, coin::CoinServiceImpl};
use academy_core_config_impl::ConfigFeatureServiceImpl;
use academy_core_contact_impl::ContactFeatureServiceImpl;
use academy_core_contract_impl::ContractFeatureServiceImpl;
use academy_core_finance_impl::{
    FinanceFeatureServiceImpl, coin::FinanceCoinServiceImpl, invoice::FinanceInvoiceServiceImpl,
};
use academy_core_health_impl::HealthFeatureServiceImpl;
use academy_core_heart_impl::{HeartFeatureServiceImpl, heart::HeartServiceImpl};
use academy_core_internal_impl::InternalServiceImpl;
use academy_core_mfa_impl::{
    MfaFeatureServiceImpl, authenticate::MfaAuthenticateServiceImpl,
    disable::MfaDisableServiceImpl, recovery::MfaRecoveryServiceImpl,
    totp_device::MfaTotpDeviceServiceImpl,
};
use academy_core_oauth2_impl::{
    OAuth2FeatureServiceImpl, authorization::OAuth2AuthorizationServiceImpl,
    link::OAuth2LinkServiceImpl, login::OAuth2LoginServiceImpl,
    registration::OAuth2RegistrationServiceImpl,
};
use academy_core_paypal_impl::{PaypalFeatureServiceImpl, coin_order::PaypalCoinOrderServiceImpl};
use academy_core_premium_impl::{
    PremiumFeatureServiceImpl, plan::PremiumPlanServiceImpl, premium::PremiumServiceImpl,
    purchase::PremiumPurchaseServiceImpl,
};
use academy_core_session_impl::{
    SessionFeatureServiceImpl, failed_auth_count::SessionFailedAuthCountServiceImpl,
    session::SessionServiceImpl,
};
use academy_core_user_impl::{
    UserFeatureServiceImpl, email_confirmation::UserEmailConfirmationServiceImpl,
    update::UserUpdateServiceImpl, user::UserServiceImpl,
};
use academy_core_withdrawal_impl::{
    WithdrawalFeatureServiceImpl, consent::WithdrawalConsentServiceImpl,
};
use academy_email_impl::{EmailServiceImpl, template::TemplateEmailServiceImpl};
use academy_extern_impl::{
    microservices::MicroservicesApiServiceImpl, oauth2::OAuth2ApiServiceImpl,
    paypal::PaypalApiServiceImpl, recaptcha::RecaptchaApiServiceImpl, render::RenderApiServiceImpl,
    vat::VatApiServiceImpl,
};
use academy_persistence_postgres::{
    PostgresDatabase, coin::PostgresCoinRepository, contract::PostgresContractRepository,
    heart::PostgresHeartRepository, mfa::PostgresMfaRepository, oauth2::PostgresOAuth2Repository,
    paypal::PostgresPaypalRepository, premium::PostgresPremiumRepository,
    session::PostgresSessionRepository, user::PostgresUserRepository,
    withdrawal::PostgresWithdrawalRepository,
};
use academy_shared_impl::{
    captcha::CaptchaServiceImpl, fs::FsServiceImpl, hash::HashServiceImpl, id::IdServiceImpl,
    jwt::JwtServiceImpl, password::PasswordServiceImpl, secret::SecretServiceImpl,
    time::TimeServiceImpl, totp::TotpServiceImpl,
};
use academy_templates_impl::TemplateServiceImpl;

// API
pub type RestServer = academy_api_rest::RestServer<
    HealthFeature,
    ConfigFeature,
    UserFeature,
    SessionFeature,
    ContactFeature,
    ContractFeature,
    MfaFeature,
    OAuth2Feature,
    CoinFeature,
    PaypalFeature,
    FinanceFeature,
    HeartFeature,
    PremiumFeature,
    WithdrawalFeature,
    Internal,
>;

// Persistence
pub type Database = PostgresDatabase;

// Cache
pub type Cache = ValkeyCache;

// Email
pub type Email = EmailServiceImpl;
pub type TemplateEmail = TemplateEmailServiceImpl<Email, Template>;

// Extern
pub type RecaptchaApi = RecaptchaApiServiceImpl;
pub type OAuth2Api = OAuth2ApiServiceImpl;
pub type VatApi = VatApiServiceImpl;
pub type PaypalApi = PaypalApiServiceImpl;
pub type RenderApi = RenderApiServiceImpl;
pub type MicroservicesApi = MicroservicesApiServiceImpl<AuthInternal>;

// Template
pub type Template = TemplateServiceImpl;

// Shared
pub type Captcha = CaptchaServiceImpl<RecaptchaApi>;
pub type Fs = FsServiceImpl;
pub type Hash = HashServiceImpl;
pub type Id = IdServiceImpl;
pub type Jwt = JwtServiceImpl<Time>;
pub type Password = PasswordServiceImpl;
pub type Secret = SecretServiceImpl;
pub type Time = TimeServiceImpl;
pub type Totp = TotpServiceImpl<Secret, Time, Hash, Cache>;

// Repositories
pub type SessionRepo = PostgresSessionRepository;
pub type UserRepo = PostgresUserRepository;
pub type MfaRepo = PostgresMfaRepository;
pub type OAuth2Repo = PostgresOAuth2Repository;
pub type CoinRepo = PostgresCoinRepository;
pub type PaypalRepo = PostgresPaypalRepository;
pub type HeartRepo = PostgresHeartRepository;
pub type PremiumRepo = PostgresPremiumRepository;
pub type ContractRepo = PostgresContractRepository;
pub type WithdrawalRepo = PostgresWithdrawalRepository;

// Auth
pub type Auth =
    AuthServiceImpl<Time, Password, UserRepo, SessionRepo, AuthAccessToken, AuthRefreshToken>;
pub type AuthAccessToken = AuthAccessTokenServiceImpl<Jwt, Cache>;
pub type AuthRefreshToken = AuthRefreshTokenServiceImpl<Secret, Hash>;
pub type AuthInternal = AuthInternalServiceImpl<Jwt>;

// Core
pub type HealthFeature = HealthFeatureServiceImpl<Time, Database, Cache, Email>;

pub type ConfigFeature = ConfigFeatureServiceImpl<Captcha>;

pub type UserFeature = UserFeatureServiceImpl<
    Database,
    Auth,
    Captcha,
    VatApi,
    MicroservicesApi,
    User,
    UserEmailConfirmation,
    UserUpdate,
    Session,
    OAuth2Registration,
    UserRepo,
    CoinRepo,
>;
pub type User = UserServiceImpl<Id, Time, Password, UserRepo, OAuth2Link>;
pub type UserEmailConfirmation =
    UserEmailConfirmationServiceImpl<Auth, Secret, TemplateEmail, Cache, Password, UserRepo>;
pub type UserUpdate = UserUpdateServiceImpl<Auth, Time, Password, Session, UserRepo>;

pub type SessionFeature = SessionFeatureServiceImpl<
    Database,
    Auth,
    Captcha,
    Session,
    SessionFailedAuthCount,
    MfaAuthenticate,
    UserRepo,
    SessionRepo,
>;
pub type Session = SessionServiceImpl<Id, Time, Auth, AuthAccessToken, SessionRepo, UserRepo>;
pub type SessionFailedAuthCount = SessionFailedAuthCountServiceImpl<Hash, Cache>;

pub type ContactFeature = ContactFeatureServiceImpl<Captcha, Email>;

pub type ContractFeature = ContractFeatureServiceImpl<
    Database,
    Auth,
    Id,
    Time,
    Cache,
    Hash,
    TemplateEmail,
    Email,
    UserRepo,
    PremiumRepo,
    ContractRepo,
>;

pub type MfaFeature = MfaFeatureServiceImpl<
    Database,
    Auth,
    UserRepo,
    MfaRepo,
    MfaRecovery,
    MfaDisable,
    MfaTotpDevice,
>;
pub type MfaRecovery = MfaRecoveryServiceImpl<Secret, Hash, MfaRepo>;
pub type MfaAuthenticate = MfaAuthenticateServiceImpl<Hash, Totp, MfaDisable, MfaRepo>;
pub type MfaDisable = MfaDisableServiceImpl<MfaRepo>;
pub type MfaTotpDevice = MfaTotpDeviceServiceImpl<Id, Time, Totp, MfaRepo>;

pub type OAuth2Feature = OAuth2FeatureServiceImpl<
    Database,
    Auth,
    UserRepo,
    OAuth2Repo,
    OAuth2Link,
    OAuth2Authorization,
    OAuth2Login,
    OAuth2Registration,
    Session,
>;
pub type OAuth2Link = OAuth2LinkServiceImpl<Id, Time, OAuth2Repo>;
pub type OAuth2Authorization = OAuth2AuthorizationServiceImpl<Secret, Cache, OAuth2Api>;
pub type OAuth2Login = OAuth2LoginServiceImpl<OAuth2Api>;
pub type OAuth2Registration = OAuth2RegistrationServiceImpl<Secret, Cache>;

pub type CoinFeature =
    CoinFeatureServiceImpl<Database, Auth, UserRepo, CoinRepo, Coin, FinanceCoin>;
pub type Coin = CoinServiceImpl<Id, Time, CoinRepo>;

pub type PaypalFeature = PaypalFeatureServiceImpl<
    Database,
    Auth,
    PaypalApi,
    UserRepo,
    PaypalRepo,
    PaypalCoinOrder,
    TemplateEmail,
    FinanceInvoice,
    FinanceCoin,
>;
pub type PaypalCoinOrder = PaypalCoinOrderServiceImpl<Time, PaypalRepo, Coin>;

pub type FinanceFeature = FinanceFeatureServiceImpl<Database, Auth, Jwt, FinanceInvoice>;
pub type FinanceInvoice = FinanceInvoiceServiceImpl<
    Time,
    Fs,
    Template,
    RenderApi,
    PaypalRepo,
    UserRepo,
    CoinRepo,
    FinanceCoin,
>;
pub type FinanceCoin = FinanceCoinServiceImpl;

pub type HeartFeature =
    HeartFeatureServiceImpl<Database, Auth, UserRepo, Heart, Coin, WithdrawalConsent>;
pub type Heart = HeartServiceImpl<Time, HeartRepo>;

pub type PremiumFeature = PremiumFeatureServiceImpl<
    Database,
    Auth,
    PremiumPlan,
    Premium,
    PremiumPurchase,
    UserRepo,
    PremiumRepo,
    WithdrawalConsent,
>;
pub type PremiumPlan = PremiumPlanServiceImpl;
pub type Premium = PremiumServiceImpl<Time, PremiumPurchase, PremiumRepo>;
pub type PremiumPurchase = PremiumPurchaseServiceImpl<Id, Time, Coin, PremiumPlan, PremiumRepo>;

pub type WithdrawalFeature = WithdrawalFeatureServiceImpl<Database, Auth, WithdrawalConsent>;
pub type WithdrawalConsent = WithdrawalConsentServiceImpl<Id, Time, WithdrawalRepo>;

pub type Internal = InternalServiceImpl<Database, AuthInternal, UserRepo, Coin, Heart, Premium>;
