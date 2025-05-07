use std::sync::LazyLock;

use academy_demo::{
    UUID1,
    oauth2::FOO_OAUTH2_LINK_1,
    user::{ADMIN, ADMIN2, ALL_USERS, BAR, FOO},
};
use academy_models::user::{User, UserComposite, UserDetails, UserFilter};
use academy_persistence_contracts::{
    Database, Transaction,
    user::{UserRepoError, UserRepository},
};
use academy_persistence_postgres::user::PostgresUserRepository;
use academy_utils::{assert_matches, patch::Patch};

use crate::{
    common::setup,
    repos::{make_slice, sliced},
};

const REPO: PostgresUserRepository = PostgresUserRepository;

macro_rules! filter {
    ($($key:ident: $value:expr),* $(,)?) => {
        UserFilter {
            $( $key: Some(TryFrom::try_from($value).unwrap()), )*
            ..Default::default()
        }
    };
}

static FILTER_TESTS: LazyLock<Vec<(UserFilter, Vec<&UserComposite>)>> = LazyLock::new(|| {
    vec![
        (filter!(), ALL_USERS.clone()),
        (filter!(name: "A"), vec![&ADMIN, &ADMIN2, &BAR]),
        (filter!(name: ""), ALL_USERS.clone()),
        (filter!(name: "does not exist"), vec![]),
        (filter!(name: "administrator"), vec![&ADMIN, &ADMIN2]),
        (filter!(name: " "), vec![&FOO]),
        (filter!(email: "EXAMPLE.com"), vec![&ADMIN, &ADMIN2, &FOO]),
        (filter!(email: ""), vec![&ADMIN, &ADMIN2, &FOO]),
        (filter!(email: "admin"), vec![&ADMIN, &ADMIN2]),
        (filter!(email_verified: true), vec![&ADMIN, &ADMIN2, &FOO]),
        (filter!(email_verified: false), vec![&BAR]),
        (filter!(admin: true), vec![&ADMIN, &ADMIN2]),
        (filter!(admin: false), vec![&FOO, &BAR]),
        (filter!(enabled: true), vec![&ADMIN, &ADMIN2, &FOO]),
        (filter!(enabled: false), vec![&BAR]),
        (filter!(mfa_enabled: true), vec![&ADMIN2]),
        (filter!(mfa_enabled: false), vec![&ADMIN, &FOO, &BAR]),
        (filter!(newsletter: true), vec![&ADMIN2, &FOO]),
        (filter!(newsletter: false), vec![&ADMIN, &BAR]),
        (filter!(admin: false, enabled: true), vec![&FOO]),
        (filter!(name: "2", admin: true), vec![&ADMIN2]),
    ]
});

#[tokio::test]
async fn count() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    for (filter, expected) in &*FILTER_TESTS {
        let count = REPO.count(&mut txn, filter).await.unwrap();
        assert_eq!(count, expected.len() as u64);
    }
}

#[tokio::test]
async fn list_composites() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    for (filter, expected) in &*FILTER_TESTS {
        let slice = make_slice(100, 0);
        let result = REPO.list_composites(&mut txn, filter, slice).await.unwrap();
        assert_eq!(&result.iter().collect::<Vec<_>>(), sliced(expected, slice));

        let slice = make_slice(2, 0);
        let result = REPO.list_composites(&mut txn, filter, slice).await.unwrap();
        assert_eq!(&result.iter().collect::<Vec<_>>(), sliced(expected, slice));

        let slice = make_slice(100, 1);
        let result = REPO.list_composites(&mut txn, filter, slice).await.unwrap();
        assert_eq!(&result.iter().collect::<Vec<_>>(), sliced(expected, slice));
    }
}

#[tokio::test]
async fn exists() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    for &user in &*ALL_USERS {
        let result = REPO.exists(&mut txn, user.user.id).await.unwrap();
        assert!(result);
    }

    let result = REPO.exists(&mut txn, UUID1.into()).await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn get_composite() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    for &user in &*ALL_USERS {
        let result = REPO
            .get_composite(&mut txn, user.user.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&result, user);
    }

    let result = REPO.get_composite(&mut txn, UUID1.into()).await.unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn get_composite_by_name() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    for &user in &*ALL_USERS {
        let result = REPO
            .get_composite_by_name(&mut txn, &user.user.name)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&result, user);
    }

    let result = REPO
        .get_composite_by_name(&mut txn, &"doesnotexist".try_into().unwrap())
        .await
        .unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn get_composite_by_email() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    for &user in &*ALL_USERS {
        let Some(email) = &user.user.email else {
            continue;
        };
        let result = REPO
            .get_composite_by_email(&mut txn, email)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&result, user);
    }

    let result = REPO
        .get_composite_by_email(&mut txn, &"doesnotexist@example.com".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn get_composite_by_oauth2_provider_id_and_remote_user_id() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    let result = REPO
        .get_composite_by_oauth2_provider_id_and_remote_user_id(
            &mut txn,
            &FOO_OAUTH2_LINK_1.provider_id,
            &FOO_OAUTH2_LINK_1.remote_user.id,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, *FOO);

    let result = REPO
        .get_composite_by_oauth2_provider_id_and_remote_user_id(
            &mut txn,
            &FOO_OAUTH2_LINK_1.provider_id,
            &"some-other-id".try_into().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn create_success() {
    let db = setup().await;

    let user_composite = UserComposite {
        user: User {
            id: UUID1.into(),
            name: "test".try_into().unwrap(),
            email: Some("test@example.com".parse().unwrap()),
            ..FOO.user.clone()
        },
        details: UserDetails {
            mfa_enabled: false,
            password_login: false,
            oauth2_login: false,
        },
        ..FOO.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(
        &mut txn,
        &user_composite.user,
        &user_composite.profile,
        &user_composite.invoice_info,
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get_composite(&mut txn, user_composite.user.id)
            .await
            .unwrap()
            .unwrap(),
        user_composite
    );
}

#[tokio::test]
async fn create_name_conflict() {
    let db = setup().await;

    let user_composite = UserComposite {
        user: User {
            id: UUID1.into(),
            email: Some("test@example.com".parse().unwrap()),
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .create(
            &mut txn,
            &user_composite.user,
            &user_composite.profile,
            &user_composite.invoice_info,
        )
        .await
        .unwrap_err();
    assert_matches!(result, UserRepoError::NameConflict);
}

#[tokio::test]
async fn create_email_conflict() {
    let db = setup().await;

    let user_composite = UserComposite {
        user: User {
            id: UUID1.into(),
            name: "test".try_into().unwrap(),
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .create(
            &mut txn,
            &user_composite.user,
            &user_composite.profile,
            &user_composite.invoice_info,
        )
        .await
        .unwrap_err();
    assert_matches!(result, UserRepoError::EmailConflict);
}

#[tokio::test]
async fn update_user() {
    let db = setup().await;

    let expected = UserComposite {
        user: User {
            id: BAR.user.id,
            name: "othername".try_into().unwrap(),
            email: Some("other@email".parse().unwrap()),
            created_at: BAR.user.created_at,
            ..FOO.user.clone()
        },
        ..BAR.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .update(&mut txn, BAR.user.id, expected.user.as_patch_ref())
        .await
        .unwrap();
    assert!(result);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_composite(&mut txn, BAR.user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, expected);
}

#[tokio::test]
async fn update_user_name_conflict() {
    let db = setup().await;

    let expected = UserComposite {
        user: User {
            id: BAR.user.id,
            email: Some("other@email".parse().unwrap()),
            ..FOO.user.clone()
        },
        ..BAR.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .update(&mut txn, BAR.user.id, expected.user.as_patch_ref())
        .await;
    assert_matches!(result, Err(UserRepoError::NameConflict));
}

#[tokio::test]
async fn update_user_email_conflict() {
    let db = setup().await;

    let expected = UserComposite {
        user: User {
            id: BAR.user.id,
            name: "othername".try_into().unwrap(),
            ..FOO.user.clone()
        },
        ..BAR.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .update(&mut txn, BAR.user.id, expected.user.as_patch_ref())
        .await;
    assert_matches!(result, Err(UserRepoError::EmailConflict));
}

#[tokio::test]
async fn update_profile() {
    let db = setup().await;

    let expected = UserComposite {
        profile: FOO.profile.clone(),
        ..BAR.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .update_profile(&mut txn, BAR.user.id, expected.profile.as_patch_ref())
        .await
        .unwrap();
    assert!(result);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_composite(&mut txn, BAR.user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, expected);
}

#[tokio::test]
async fn update_invoice_info() {
    let db = setup().await;

    let expected = UserComposite {
        invoice_info: FOO.invoice_info.clone(),
        ..BAR.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .update_invoice_info(&mut txn, BAR.user.id, expected.invoice_info.as_patch_ref())
        .await
        .unwrap();
    assert!(result);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_composite(&mut txn, BAR.user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, expected);
}

#[tokio::test]
async fn delete() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.delete(&mut txn, FOO.user.id).await.unwrap();
    assert!(result);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_composite(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, None);

    let result = REPO.delete(&mut txn, FOO.user.id).await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn password() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.save_password_hash(&mut txn, FOO.user.id, "the password hash".into())
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_password_hash(&mut txn, FOO.user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, "the password hash");

    REPO.save_password_hash(&mut txn, FOO.user.id, "some other password hash".into())
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_password_hash(&mut txn, FOO.user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, "some other password hash");

    let result = REPO
        .remove_password_hash(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert!(result);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_password_hash(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn user_numbers() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    assert_eq!(REPO.get_number(&mut txn, FOO.user.id).await.unwrap(), 1);
    assert_eq!(REPO.get_number(&mut txn, FOO.user.id).await.unwrap(), 1);

    assert_eq!(REPO.get_number(&mut txn, BAR.user.id).await.unwrap(), 2);

    assert_eq!(REPO.get_number(&mut txn, FOO.user.id).await.unwrap(), 1);

    assert_eq!(REPO.get_number(&mut txn, BAR.user.id).await.unwrap(), 2);
}
