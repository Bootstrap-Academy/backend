use super::Sut;
use academy_auth_contracts::internal::MockAuthInternalService;
use academy_core_internal_contracts::{InternalHasPremiumError, InternalService};
use academy_demo::user::FOO;
use academy_models::learning_policy::{LearningMode, LearningPolicyConfig};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};

pub(super) fn daily_config() -> LearningPolicyConfig {
    LearningPolicyConfig {
        mode: LearningMode::Daily,
        terms_version: Some("approved-test-terms".try_into().unwrap()),
        accepted_since: Some(FOO.user.created_at),
        user_ids: vec![FOO.user.id],
        registered_since: None,
        daily_documents: None,
    }
}

#[test]
fn daily_requires_both_cohort_and_matching_recorded_acceptance() {
    let mut user = FOO.user.clone();
    user.terms_accepted_at = None;
    let mut config = daily_config();
    assert_eq!(config.mode_for(&user), LearningMode::Legacy);
    user.terms_version = config.terms_version.clone();
    assert_eq!(config.mode_for(&user), LearningMode::Legacy);
    user.terms_accepted_at = config.accepted_since;
    assert_eq!(config.mode_for(&user), LearningMode::Daily);
    config.user_ids.clear();
    assert_eq!(config.mode_for(&user), LearningMode::Legacy);
    config.registered_since = Some(user.created_at);
    assert_eq!(config.mode_for(&user), LearningMode::Daily);
    config.registered_since = Some(user.created_at + std::time::Duration::from_secs(1));
    assert_eq!(config.mode_for(&user), LearningMode::Legacy);
    config.user_ids.push(user.id);
    config.accepted_since = Some(user.created_at + std::time::Duration::from_secs(1));
    assert_eq!(config.mode_for(&user), LearningMode::Legacy);
    config.mode = LearningMode::Shadow;
    assert_eq!(config.mode_for(&user), LearningMode::Shadow);
    config.mode = LearningMode::Legacy;
    assert_eq!(config.mode_for(&user), LearningMode::Legacy);
}

#[tokio::test]
async fn policy_reads_paid_status_without_renewal_for_all_modes() {
    for mode in [
        LearningMode::Legacy,
        LearningMode::Shadow,
        LearningMode::Daily,
    ] {
        let mut config = daily_config();
        config.mode = mode;
        let mut user = FOO.clone();
        user.user.terms_version = config.terms_version.clone();
        user.user.terms_accepted_at = config.accepted_since;
        let mut sut = Sut {
            db: MockDatabase::build(false),
            auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
            user_repo: MockUserRepository::new()
                .with_get_internal_composite(FOO.user.id, Some(user)),
            learning_policy_config: config,
            ..Sut::default()
        };
        sut.premium
            .expect_get_current()
            .once()
            .return_once(|_, _| Box::pin(async { Ok(None) }));
        let policy = sut
            .learning_policy(&"internal token".into(), FOO.user.id)
            .await
            .unwrap();
        assert_eq!(policy.mode, mode);
        assert!(!policy.premium);
        assert_eq!(policy.heart_sales, mode != LearningMode::Daily);
        assert_eq!(policy.single_course_sales, mode != LearningMode::Daily);
    }
}

#[tokio::test]
async fn premium_failure_is_an_error_never_a_free_tier_policy() {
    let mut sut = Sut {
        db: MockDatabase::build(false),
        auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
        user_repo: MockUserRepository::new()
            .with_get_internal_composite(FOO.user.id, Some(FOO.clone())),
        ..Sut::default()
    };
    sut.premium
        .expect_get_current()
        .once()
        .return_once(|_, _| Box::pin(async { Err(anyhow::anyhow!("synthetic database failure")) }));
    assert!(matches!(
        sut.learning_policy(&"internal token".into(), FOO.user.id)
            .await,
        Err(InternalHasPremiumError::Other(_))
    ));
}

#[tokio::test]
async fn authentication_and_missing_subject_fail_without_premium_lookup() {
    let sut = Sut {
        auth_internal: MockAuthInternalService::new().with_authenticate("shop", false),
        ..Sut::default()
    };
    assert!(matches!(
        sut.learning_policy(&"internal token".into(), FOO.user.id)
            .await,
        Err(InternalHasPremiumError::Auth(_))
    ));
    let sut = Sut {
        db: MockDatabase::build(false),
        auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
        user_repo: MockUserRepository::new().with_get_internal_composite(FOO.user.id, None),
        ..Sut::default()
    };
    assert!(matches!(
        sut.learning_policy(&"internal token".into(), FOO.user.id)
            .await,
        Err(InternalHasPremiumError::UserNotFound)
    ));
}
