use std::{sync::LazyLock, time::Duration};

use academy_models::session::Session;
use academy_persistence_contracts::session::SessionRepository;
use uuid::uuid;

use crate::user::{ADMIN, BAR, FOO};

pub static ALL_SESSIONS: LazyLock<Vec<&Session>> = LazyLock::new(|| vec![&ADMIN_1, &FOO_1, &FOO_2]);

pub static ADMIN_1: LazyLock<Session> = LazyLock::new(|| Session {
    id: uuid!("1943a975-8895-428d-9fb1-f8d450f29dae").into(),
    user_id: ADMIN.user.id,
    device_name: Some("laptop".try_into().unwrap()),
    created_at: ADMIN.user.created_at,
    updated_at: ADMIN.user.created_at + Duration::from_secs(1337),
    // administrative privileges require a session created with a second factor
    mfa_verified: true,
});

pub static FOO_1: LazyLock<Session> = LazyLock::new(|| Session {
    id: uuid!("b2b772de-4fc6-4651-9684-c71e70b9197b").into(),
    user_id: FOO.user.id,
    device_name: Some("desktop".try_into().unwrap()),
    created_at: FOO.user.created_at + Duration::from_secs(42),
    updated_at: FOO.user.created_at + Duration::from_secs(1337),
    mfa_verified: false,
});

pub static FOO_2: LazyLock<Session> = LazyLock::new(|| Session {
    id: uuid!("eb0fe09a-552e-40c1-a912-e77ec9ca8b36").into(),
    user_id: FOO.user.id,
    device_name: None,
    created_at: FOO.user.created_at,
    updated_at: FOO.user.created_at + Duration::from_secs(17),
    mfa_verified: false,
});

pub static BAR_1: LazyLock<Session> = LazyLock::new(|| Session {
    id: uuid!("2dbe3650-aad6-412a-9207-68a444697909").into(),
    user_id: BAR.user.id,
    device_name: None,
    created_at: BAR.user.created_at,
    updated_at: BAR.user.created_at + Duration::from_secs(23),
    mfa_verified: false,
});

pub async fn create<Txn: Send + Sync + 'static>(
    txn: &mut Txn,
    repo: impl SessionRepository<Txn>,
) -> anyhow::Result<()> {
    for &session in &*ALL_SESSIONS {
        repo.create(txn, session).await?;
    }
    Ok(())
}
