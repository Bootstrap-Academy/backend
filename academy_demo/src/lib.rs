use std::sync::LazyLock;

use academy_models::{Sha256Hash, VerificationCode};
use academy_persistence_contracts::{
    mfa::MfaRepository, oauth2::OAuth2Repository, session::SessionRepository, user::UserRepository,
};
use anyhow::Context;
use uuid::{Uuid, uuid};

pub mod mfa;
pub mod oauth2;
pub mod session;
pub mod user;

pub const UUID1: Uuid = uuid!("eb1cd87a-4475-4d68-a2c2-0216bdaac8f7");
pub const UUID2: Uuid = uuid!("316c8e26-4b07-4795-ab40-b28d8bf8e493");

pub const SHA256HASH1_HEX: &str =
    "4a1df3d808c2fe0882ec627549102fa62ca4357ac00874e2d9754b98b34e5ad6";
pub const SHA256HASH2_HEX: &str =
    "19e03e14115709547dccd3f853180caf6d87605ad4be173402b1e1e0389e5ef3";

pub static SHA256HASH1: LazyLock<Sha256Hash> = LazyLock::new(|| sha256hash(SHA256HASH1_HEX));
pub static SHA256HASH2: LazyLock<Sha256Hash> = LazyLock::new(|| sha256hash(SHA256HASH2_HEX));

fn sha256hash(hash: &str) -> Sha256Hash {
    Sha256Hash(hex::decode(hash).unwrap().try_into().unwrap())
}

pub static VERIFICATION_CODE_1: LazyLock<VerificationCode> =
    LazyLock::new(|| "UH86-I3DC-PWPP-VKQ9".try_into().unwrap());
pub static VERIFICATION_CODE_2: LazyLock<VerificationCode> =
    LazyLock::new(|| "HFWG-6TTY-0UY4-73YZ".try_into().unwrap());

pub async fn create<Txn: Send + Sync + 'static>(
    txn: &mut Txn,
    user: impl UserRepository<Txn>,
    session: impl SessionRepository<Txn>,
    mfa: impl MfaRepository<Txn>,
    oauth2: impl OAuth2Repository<Txn>,
) -> anyhow::Result<()> {
    macro_rules! create {
        ($($ident:ident),* $(,)?) => { $(
            $ident::create(txn, $ident).await.context(concat!(
                "Failed to create ", stringify!(ident), " demo data"
            ))?;
        )*};
    }

    create!(user, session, mfa, oauth2);

    Ok(())
}
