//! Type aliases for production implementations of all service traits

use academy_auth_impl::{
    access_token::AuthAccessTokenServiceImpl, internal::AuthInternalServiceImpl,
    refresh_token::AuthRefreshTokenServiceImpl, AuthServiceImpl,
};
use academy_cache_valkey::ValkeyCache;
use academy_core_coin_impl::{coin::CoinServiceImpl, CoinFeatureServiceImpl};
use academy_core_config_impl::ConfigFeatureServiceImpl;
use academy_core_contact_impl::ContactFeatureServiceImpl;
use academy_core_health_impl::HealthFeatureServiceImpl;
use academy_core_internal_impl::InternalServiceImpl;
use academy_core_mfa_impl::{
    authenticate::MfaAuthenticateServiceImpl, disable::MfaDisableServiceImpl,
    recovery::MfaRecoveryServiceImpl, totp_device::MfaTotpDeviceServiceImpl, MfaFeatureServiceImpl,
};
use academy_core_oauth2_impl::{
    link::OAuth2LinkServiceImpl, login::OAuth2LoginServiceImpl,
    registration::OAuth2RegistrationServiceImpl, OAuth2FeatureServiceImpl,
};
use academy_core_paypal_impl::{coin_order::PaypalCoinOrderServiceImpl, PaypalFeatureServiceImpl};
use academy_core_session_impl::{
    failed_auth_count::SessionFailedAuthCountServiceImpl, session::SessionServiceImpl,
    SessionFeatureServiceImpl,
};
use academy_core_user_impl::{
    email_confirmation::UserEmailConfirmationServiceImpl, update::UserUpdateServiceImpl,
    user::UserServiceImpl, UserFeatureServiceImpl,
};
use academy_email_impl::{template::TemplateEmailServiceImpl, EmailServiceImpl};
use academy_extern_impl::{
    internal::InternalApiServiceImpl, oauth2::OAuth2ApiServiceImpl, paypal::PaypalApiServiceImpl,
    recaptcha::RecaptchaApiServiceImpl, vat::VatApiServiceImpl,
};
use academy_persistence_postgres::{
    coin::PostgresCoinRepository, mfa::PostgresMfaRepository, oauth2::PostgresOAuth2Repository,
    paypal::PostgresPaypalRepository, session::PostgresSessionRepository,
    user::PostgresUserRepository, PostgresDatabase,
};
use academy_shared_impl::{
    captcha::CaptchaServiceImpl, hash::HashServiceImpl, id::IdServiceImpl, jwt::JwtServiceImpl,
    password::PasswordServiceImpl, secret::SecretServiceImpl, time::TimeServiceImpl,
    totp::TotpServiceImpl,
};
use academy_templates_impl::TemplateServiceImpl;

// API
pub type RestServer = academy_api_rest::RestServer<
    HealthFeature,
    ConfigFeature,
    UserFeature,
    SessionFeature,
    ContactFeature,
    MfaFeature,
    OAuth2Feature,
    CoinFeature,
    PaypalFeature,
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
pub type InternalApi = InternalApiServiceImpl<AuthInternal>;
pub type VatApi = VatApiServiceImpl;
pub type PaypalApi = PaypalApiServiceImpl;

// Template
pub type Template = TemplateServiceImpl;

// Shared
pub type Captcha = CaptchaServiceImpl<RecaptchaApi>;
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
    InternalApi,
    User,
    UserEmailConfirmation,
    UserUpdate,
    Session,
    OAuth2Registration,
    UserRepo,
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
    OAuth2Api,
    UserRepo,
    OAuth2Repo,
    OAuth2Link,
    OAuth2Login,
    OAuth2Registration,
    Session,
>;
pub type OAuth2Link = OAuth2LinkServiceImpl<Id, Time, OAuth2Repo>;
pub type OAuth2Login = OAuth2LoginServiceImpl<OAuth2Api>;
pub type OAuth2Registration = OAuth2RegistrationServiceImpl<Secret, Cache>;

pub type CoinFeature = CoinFeatureServiceImpl<Database, Auth, UserRepo, CoinRepo, Coin>;
pub type Coin = CoinServiceImpl<CoinRepo>;

pub type PaypalFeature =
    PaypalFeatureServiceImpl<Database, Auth, PaypalApi, UserRepo, PaypalRepo, PaypalCoinOrder>;
pub type PaypalCoinOrder = PaypalCoinOrderServiceImpl<Time, PaypalRepo, CoinRepo>;

pub type Internal = InternalServiceImpl<Database, AuthInternal, UserRepo, Coin>;
