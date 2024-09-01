use academy_cache_valkey::ValkeyCache;
use academy_core_auth_impl::{
    commands::invalidate_access_token::AuthInvalidateAccessTokenCommandServiceImpl, AuthServiceImpl,
};
use academy_core_config_impl::ConfigServiceImpl;
use academy_core_contact_impl::ContactServiceImpl;
use academy_core_health_impl::HealthServiceImpl;
use academy_core_mfa_impl::{
    commands::{
        authenticate::MfaAuthenticateCommandServiceImpl,
        confirm_totp_device::MfaConfirmTotpDeviceCommandServiceImpl,
        create_totp_device::MfaCreateTotpDeviceCommandServiceImpl,
        disable::MfaDisableCommandServiceImpl,
        reset_totp_device::MfaResetTotpDeviceCommandServiceImpl,
        setup_recovery::MfaSetupRecoveryCommandServiceImpl,
    },
    MfaServiceImpl,
};
use academy_core_session_impl::{
    commands::{
        create::SessionCreateCommandServiceImpl, delete::SessionDeleteCommandServiceImpl,
        delete_by_user::SessionDeleteByUserCommandServiceImpl,
        refresh::SessionRefreshCommandServiceImpl,
    },
    SessionServiceImpl,
};
use academy_core_user_impl::{
    commands::{
        create::UserCreateCommandServiceImpl,
        request_password_reset_email::UserRequestPasswordResetEmailCommandServiceImpl,
        request_subscribe_newsletter_email::UserRequestSubscribeNewsletterEmailCommandServiceImpl,
        request_verification_email::UserRequestVerificationEmailCommandServiceImpl,
        reset_password::UserResetPasswordCommandServiceImpl,
        update_admin::UserUpdateAdminCommandServiceImpl,
        update_email::UserUpdateEmailCommandServiceImpl,
        update_enabled::UserUpdateEnabledCommandServiceImpl,
        update_name::UserUpdateNameCommandServiceImpl,
        update_password::UserUpdatePasswordCommandServiceImpl,
        verify_email::UserVerifyEmailCommandServiceImpl,
        verify_newsletter_subscription::UserVerifyNewsletterSubscriptionCommandServiceImpl,
    },
    queries::{
        get_by_name_or_email::UserGetByNameOrEmailQueryServiceImpl, list::UserListQueryServiceImpl,
    },
    UserServiceImpl,
};
use academy_email_impl::{template::TemplateEmailServiceImpl, EmailServiceImpl};
use academy_extern_impl::recaptcha::RecaptchaApiServiceImpl;
use academy_persistence_postgres::{
    mfa::PostgresMfaRepository, session::PostgresSessionRepository, user::PostgresUserRepository,
    PostgresDatabase,
};
use academy_shared_impl::{
    captcha::CaptchaServiceImpl, hash::HashServiceImpl, id::IdServiceImpl, jwt::JwtServiceImpl,
    password::PasswordServiceImpl, secret::SecretServiceImpl, time::TimeServiceImpl,
    totp::TotpServiceImpl,
};
use academy_templates_impl::TemplateServiceImpl;

// API
pub type RestServer = academy_api_rest::RestServer<Health, Config, User, Session, Contact, Mfa>;

// Persistence
pub type Database = PostgresDatabase;

// Cache
pub type Cache = ValkeyCache;

// Email
pub type Email = EmailServiceImpl;
pub type TemplateEmail = TemplateEmailServiceImpl<Email, Template>;

// Extern
pub type RecaptchaApi = RecaptchaApiServiceImpl;

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

// Core
pub type Auth = AuthServiceImpl<
    Jwt,
    Secret,
    Time,
    Hash,
    Password,
    UserRepo,
    SessionRepo,
    Cache,
    AuthInvalidateAccessToken,
>;
pub type AuthInvalidateAccessToken = AuthInvalidateAccessTokenCommandServiceImpl<Cache>;

pub type Health = HealthServiceImpl<Time, Database, Cache, Email>;

pub type Config = ConfigServiceImpl<Captcha>;

pub type User = UserServiceImpl<
    Database,
    Auth,
    Captcha,
    UserList,
    UserCreate,
    UserRequestSubscribeNewsletterEmail,
    UserUpdateName,
    UserUpdateEmail,
    UserUpdateAdmin,
    UserUpdateEnabled,
    UserUpdatePassword,
    UserVerifyNewsletterSubscription,
    UserRequestVerificationEmail,
    UserVerifyEmail,
    UserRequestPasswordResetEmail,
    UserResetPassword,
    SessionCreate,
    UserRepo,
>;
pub type UserCreate = UserCreateCommandServiceImpl<Id, Time, Password, UserRepo>;
pub type UserRequestSubscribeNewsletterEmail =
    UserRequestSubscribeNewsletterEmailCommandServiceImpl<Secret, TemplateEmail, Cache>;
pub type UserUpdateName = UserUpdateNameCommandServiceImpl<Time, UserRepo>;
pub type UserUpdateEmail = UserUpdateEmailCommandServiceImpl<Auth, UserRepo>;
pub type UserUpdateAdmin = UserUpdateAdminCommandServiceImpl<Auth, UserRepo>;
pub type UserUpdateEnabled = UserUpdateEnabledCommandServiceImpl<UserRepo, SessionDeleteByUser>;
pub type UserUpdatePassword = UserUpdatePasswordCommandServiceImpl<Password, UserRepo>;
pub type UserGetByNameOrEmail = UserGetByNameOrEmailQueryServiceImpl<UserRepo>;
pub type UserVerifyNewsletterSubscription =
    UserVerifyNewsletterSubscriptionCommandServiceImpl<UserRepo, Cache>;
pub type UserRequestVerificationEmail =
    UserRequestVerificationEmailCommandServiceImpl<Secret, TemplateEmail, Cache>;
pub type UserRequestPasswordResetEmail =
    UserRequestPasswordResetEmailCommandServiceImpl<Secret, TemplateEmail, Cache>;
pub type UserResetPassword = UserResetPasswordCommandServiceImpl<Cache, Password, UserRepo>;
pub type UserVerifyEmail = UserVerifyEmailCommandServiceImpl<Auth, Cache, UserRepo>;
pub type UserList = UserListQueryServiceImpl<UserRepo>;

pub type Session = SessionServiceImpl<
    Database,
    Auth,
    SessionCreate,
    SessionRefresh,
    SessionDelete,
    SessionDeleteByUser,
    UserGetByNameOrEmail,
    MfaAuthenticate,
    UserRepo,
    SessionRepo,
>;
pub type SessionCreate = SessionCreateCommandServiceImpl<Id, Time, Auth, SessionRepo, UserRepo>;
pub type SessionRefresh = SessionRefreshCommandServiceImpl<Time, Auth, UserRepo, SessionRepo>;
pub type SessionDelete = SessionDeleteCommandServiceImpl<Auth, SessionRepo>;
pub type SessionDeleteByUser = SessionDeleteByUserCommandServiceImpl<Auth, SessionRepo>;

pub type Contact = ContactServiceImpl<Captcha, Email>;

pub type Mfa = MfaServiceImpl<
    Database,
    Auth,
    UserRepo,
    MfaRepo,
    MfaCreateTotpDevice,
    MfaResetTotpDevice,
    MfaConfirmTotpDevice,
    MfaSetupRecovery,
    MfaDisable,
>;
pub type MfaCreateTotpDevice = MfaCreateTotpDeviceCommandServiceImpl<Id, Time, Totp, MfaRepo>;
pub type MfaResetTotpDevice = MfaResetTotpDeviceCommandServiceImpl<Totp, MfaRepo>;
pub type MfaConfirmTotpDevice = MfaConfirmTotpDeviceCommandServiceImpl<Totp, MfaRepo>;
pub type MfaSetupRecovery = MfaSetupRecoveryCommandServiceImpl<Secret, Hash, MfaRepo>;
pub type MfaAuthenticate = MfaAuthenticateCommandServiceImpl<Hash, Totp, MfaDisable, MfaRepo>;
pub type MfaDisable = MfaDisableCommandServiceImpl<MfaRepo>;
