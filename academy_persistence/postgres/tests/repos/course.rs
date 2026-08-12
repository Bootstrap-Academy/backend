use academy_demo::user::FOO;
use academy_models::course::{CourseId, CourseUser, CourseUserPatchRef};
use academy_persistence_contracts::{Database, Transaction, course::CourseRepository};
use academy_persistence_postgres::course::PostgresCourseRepository;
use academy_utils::patch::PatchValue;

use crate::common::setup;

const REPO: PostgresCourseRepository = PostgresCourseRepository;

#[tokio::test]
async fn course_users() {
    let db = setup().await;

    let course_id = CourseId::new("course");

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_course_user(&mut txn, &course_id, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(
        result,
        CourseUser {
            course_id: course_id.clone(),
            user_id: FOO.user.id,
            purchased: false
        }
    );

    REPO.update_course_user(
        &mut txn,
        &course_id,
        FOO.user.id,
        CourseUserPatchRef {
            purchased: PatchValue::Update(&true),
        },
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_course_user(&mut txn, &course_id, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(
        result,
        CourseUser {
            course_id: course_id.clone(),
            user_id: FOO.user.id,
            purchased: true
        }
    );

    REPO.update_course_user(
        &mut txn,
        &course_id,
        FOO.user.id,
        CourseUserPatchRef {
            purchased: PatchValue::Update(&false),
        },
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_course_user(&mut txn, &course_id, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(
        result,
        CourseUser {
            course_id: course_id.clone(),
            user_id: FOO.user.id,
            purchased: false
        }
    );
}
