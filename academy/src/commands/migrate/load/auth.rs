use academy_models::{
    Sha256Hash,
    mfa::{MfaRecoveryCodeHash, TotpDevice, TotpSecret},
    oauth2::{OAuth2Link, OAuth2UserInfo},
    session::{Session, SessionRefreshTokenHash},
    user::{User, UserInvoiceInfo, UserProfile},
};
use academy_persistence_contracts::{
    Database, Transaction, mfa::MfaRepository, oauth2::OAuth2Repository,
    session::SessionRepository, user::UserRepository,
};
use academy_persistence_postgres::{
    PostgresDatabase, mfa::PostgresMfaRepository, oauth2::PostgresOAuth2Repository,
    session::PostgresSessionRepository, user::PostgresUserRepository,
};
use academy_shared_contracts::hash::HashService;
use academy_shared_impl::hash::HashServiceImpl;
use chrono::NaiveDateTime;
use indicatif::ProgressIterator;
use tracing::info;
use uuid::Uuid;

use super::DbConnection;

pub async fn load(db: PostgresDatabase, auth: DbConnection) -> anyhow::Result<()> {
    let mut txn = db.begin_transaction().await?;

    let user_repo = PostgresUserRepository;
    let mfa_repo = PostgresMfaRepository;
    let session_repo = PostgresSessionRepository;
    let oauth2_repo = PostgresOAuth2Repository;

    info!("loading users");
    for row in auth
        .query("select * from auth_user order by registration asc", &[])
        .await?
        .into_iter()
        .progress()
    {
        let id: String = row.get("id");
        let name: String = row.get("name");
        let password: Option<String> = row.get("password");
        let registration: NaiveDateTime = row.get("registration");
        let last_login: Option<NaiveDateTime> = row.get("last_login");
        let enabled: bool = row.get("enabled");
        let admin: bool = row.get("admin");
        let mfa_secret: Option<String> = row.get("mfa_secret");
        let mfa_enabled: Option<bool> = row.get("mfa_enabled");
        let mfa_recovery_code: Option<String> = row.get("mfa_recovery_code");
        let display_name: String = row.get("display_name");
        let email: Option<String> = row.get("email");
        let email_verification_code: Option<String> = row.get("email_verification_code");
        let description: Option<String> = row.get("description");
        let tags: Option<String> = row.get("_tags");
        let last_name_change: Option<NaiveDateTime> = row.get("last_name_change");
        let business: Option<bool> = row.get("business");
        let first_name: Option<String> = row.get("first_name");
        let last_name: Option<String> = row.get("last_name");
        let street: Option<String> = row.get("street");
        let zip_code: Option<String> = row.get("zip_code");
        let city: Option<String> = row.get("city");
        let country: Option<String> = row.get("country");
        let vat_id: Option<String> = row.get("vat_id");

        let user = User {
            id: id.parse::<Uuid>()?.into(),
            name: name.try_into()?,
            email: email.map(|x| x.parse()).transpose()?,
            email_verified: email_verification_code.is_none(),
            created_at: registration.and_utc(),
            last_login: last_login.map(|x| x.and_utc()),
            last_name_change: last_name_change
                .map(|x| x.and_utc())
                .filter(|x| x.timestamp() != 0),
            enabled,
            admin,
            terms_version: None,
            terms_accepted_at: None,
            age_confirmed_at: None,
            terms_declined_at: None,
        };

        let profile = UserProfile {
            display_name: display_name.try_into()?,
            bio: description
                .map(TryInto::try_into)
                .transpose()?
                .unwrap_or_default(),
            tags: serde_json::from_str(tags.as_deref().unwrap_or("[]"))?,
            leaderboard_opt_out: false,
        };

        let invoice_info = UserInvoiceInfo {
            business,
            first_name: first_name.map(TryInto::try_into).transpose()?,
            last_name: last_name.map(TryInto::try_into).transpose()?,
            street: street.map(TryInto::try_into).transpose()?,
            zip_code: zip_code.map(TryInto::try_into).transpose()?,
            city: city.map(TryInto::try_into).transpose()?,
            country: country.map(TryInto::try_into).transpose()?,
            vat_id: vat_id.map(TryInto::try_into).transpose()?,
        };

        user_repo
            .create(&mut txn, &user, &profile, &invoice_info)
            .await?;

        if let Some(hash) = password {
            user_repo
                .save_password_hash(&mut txn, user.id, hash)
                .await?;
        }

        if let Some(mfa_secret) = mfa_secret {
            let totp_device = TotpDevice {
                id: Uuid::new_v4().into(),
                user_id: user.id,
                enabled: mfa_enabled.unwrap_or(false),
                created_at: user.created_at,
            };
            let secret = TotpSecret::try_new(
                base32::decode(base32::Alphabet::Rfc4648 { padding: false }, &mfa_secret)
                    .ok_or_else(|| anyhow::anyhow!("Failed to decode totp secret"))?,
            )?;

            mfa_repo
                .create_totp_device(&mut txn, &totp_device, &secret)
                .await?;
        }

        if let Some(hash) = mfa_recovery_code {
            let hash = if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                Sha256Hash(hex::decode(&hash).unwrap().try_into().unwrap())
            } else {
                HashServiceImpl.sha256(&hash)
            };

            let hash = MfaRecoveryCodeHash::new(hash);
            mfa_repo
                .save_mfa_recovery_code_hash(&mut txn, user.id, hash)
                .await?;
        }
    }

    info!("loading sessions");
    for row in auth
        .query("select * from auth_session", &[])
        .await?
        .into_iter()
        .progress()
    {
        let id: String = row.get("id");
        let user_id: String = row.get("user_id");
        let device_name: String = row.get("device_name");
        let last_update: NaiveDateTime = row.get("last_update");
        let refresh_token: String = row.get("refresh_token");

        let session = Session {
            id: id.parse::<Uuid>()?.into(),
            user_id: user_id.parse::<Uuid>()?.into(),
            device_name: Some(device_name.try_into()?),
            created_at: last_update.and_utc(),
            updated_at: last_update.and_utc(),
            mfa_verified: false,
        };

        let refresh_token_hash = SessionRefreshTokenHash::new(Sha256Hash(
            hex::decode(refresh_token)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("Failed to decode refresh token hash"))?,
        ));

        session_repo.create(&mut txn, &session).await?;
        session_repo
            .save_refresh_token_hash(&mut txn, session.id, refresh_token_hash)
            .await?;
    }

    info!("loading oauth2 links");
    for row in auth
        .query(
            "select *, registration from auth_oauth_user_connection c inner join auth_user u on \
             u.id=c.user_id",
            &[],
        )
        .await?
        .into_iter()
        .progress()
    {
        let id: String = row.get("id");
        let user_id: String = row.get("user_id");
        let provider_id: String = row.get("provider_id");
        let remote_user_id: String = row.get("remote_user_id");
        let display_name: String = row.get("display_name");
        let registration: NaiveDateTime = row.get("registration");

        let link = OAuth2Link {
            id: id.parse::<Uuid>()?.into(),
            user_id: user_id.parse::<Uuid>()?.into(),
            provider_id: provider_id.into(),
            created_at: registration.and_utc(),
            remote_user: OAuth2UserInfo {
                id: remote_user_id.try_into()?,
                name: display_name.try_into()?,
            },
        };

        oauth2_repo.create_link(&mut txn, &link).await?;
    }

    info!("committing changes");
    txn.commit().await?;

    info!("done");

    Ok(())
}
