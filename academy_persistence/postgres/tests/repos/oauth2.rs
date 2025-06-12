use std::slice;

use academy_demo::{
    UUID1, UUID2,
    oauth2::FOO_OAUTH2_LINK_1,
    user::{BAR, FOO},
};
use academy_models::oauth2::{OAuth2Link, OAuth2UserInfo};
use academy_persistence_contracts::{
    Database, Transaction,
    oauth2::{OAuth2RepoError, OAuth2Repository},
};
use academy_persistence_postgres::oauth2::PostgresOAuth2Repository;
use academy_utils::{Apply, assert_matches};

use crate::common::setup;

const REPO: PostgresOAuth2Repository = PostgresOAuth2Repository;

#[tokio::test]
async fn list_links_by_user() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    let result = REPO
        .list_links_by_user(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, slice::from_ref(&*FOO_OAUTH2_LINK_1));
}

#[tokio::test]
async fn get_link() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    let result = REPO.get_link(&mut txn, FOO_OAUTH2_LINK_1.id).await.unwrap();
    assert_eq!(result.unwrap(), *FOO_OAUTH2_LINK_1);

    let result = REPO.get_link(&mut txn, UUID1.into()).await.unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn create_link() {
    let link = OAuth2Link {
        id: UUID1.into(),
        user_id: FOO.user.id,
        provider_id: "abc".into(),
        created_at: FOO.user.created_at,
        remote_user: OAuth2UserInfo {
            id: "test-id".try_into().unwrap(),
            name: "test-name".try_into().unwrap(),
        },
    };

    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create_link(&mut txn, &link).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .list_links_by_user(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, [FOO_OAUTH2_LINK_1.clone(), link.clone()]);

    let result = REPO
        .create_link(
            &mut txn,
            &link.with(|l| {
                l.id = UUID2.into();
                l.user_id = BAR.user.id;
            }),
        )
        .await;
    assert_matches!(result, Err(OAuth2RepoError::Conflict));
}

#[tokio::test]
async fn delete_link() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    let result = REPO
        .delete_link(&mut txn, FOO_OAUTH2_LINK_1.id)
        .await
        .unwrap();
    assert!(result);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_link(&mut txn, FOO_OAUTH2_LINK_1.id).await.unwrap();
    assert_eq!(result, None);

    let result = REPO
        .delete_link(&mut txn, FOO_OAUTH2_LINK_1.id)
        .await
        .unwrap();
    assert!(!result);
}
