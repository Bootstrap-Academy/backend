use std::{sync::LazyLock, time::Duration};

use academy_models::oauth2::{OAuth2Link, OAuth2Provider, OAuth2ProviderId, OAuth2UserInfo};
use academy_persistence_contracts::oauth2::OAuth2Repository;
use uuid::uuid;

use crate::user::FOO;

pub static TEST_OAUTH2_PROVIDER_ID: LazyLock<OAuth2ProviderId> =
    LazyLock::new(|| OAuth2ProviderId::new("test"));

pub static TEST_OAUTH2_PROVIDER: LazyLock<OAuth2Provider> = LazyLock::new(|| OAuth2Provider {
    name: "Test Provider".into(),
    client_id: "test-id".into(),
    client_secret: "test-secret".into(),
    auth_url: "http://test/auth".parse().unwrap(),
    token_url: "http://test/token".parse().unwrap(),
    userinfo_url: "http://test/user".parse().unwrap(),
    userinfo_id_key: "id".into(),
    userinfo_name_key: "name".into(),
    scopes: ["foo", "bar", "baz"].map(Into::into).into(),
});

pub static ALL_OAUTH2_LINKS: LazyLock<Vec<&OAuth2Link>> =
    LazyLock::new(|| vec![&FOO_OAUTH2_LINK_1]);

pub static FOO_OAUTH2_LINK_1: LazyLock<OAuth2Link> = LazyLock::new(|| OAuth2Link {
    id: uuid!("2cc0be1e-6c7b-4a7b-8ee9-226e15147ee5").into(),
    user_id: FOO.user.id,
    provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
    created_at: FOO.user.created_at + Duration::from_secs(28384),
    remote_user: OAuth2UserInfo {
        id: "28374".try_into().unwrap(),
        name: "Foo42".try_into().unwrap(),
    },
});

pub async fn create<Txn: Send + Sync + 'static>(
    txn: &mut Txn,
    repo: impl OAuth2Repository<Txn>,
) -> anyhow::Result<()> {
    for &link in &*ALL_OAUTH2_LINKS {
        repo.create_link(txn, link).await?;
    }
    Ok(())
}
