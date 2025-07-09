use academy_di::Build;
use academy_models::{
    course::{CourseId, CourseUser, CourseUserPatchRef},
    user::UserId,
};
use academy_persistence_contracts::course::CourseRepository;
use academy_utils::trace_instrument;
use clorinde::{
    client::Params,
    queries::{
        self,
        course::{GetCourseUserParams, UpdateCourseUserParams},
    },
};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresCourseRepository;

impl CourseRepository<PostgresTransaction> for PostgresCourseRepository {
    #[trace_instrument(skip(self, txn))]
    async fn get_course_user(
        &self,
        txn: &mut PostgresTransaction,
        course_id: &CourseId,
        user_id: UserId,
    ) -> anyhow::Result<CourseUser> {
        queries::course::get_course_user()
            .params(
                txn.txn(),
                &GetCourseUserParams {
                    course_id: &**course_id,
                    user_id: *user_id,
                },
            )
            .opt()
            .await
            .map_err(Into::into)
            .map(|row| {
                row.map(decode_course_user).unwrap_or_else(|| CourseUser {
                    course_id: course_id.clone(),
                    user_id,
                    purchased: false,
                })
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn update_course_user<'a>(
        &self,
        txn: &mut PostgresTransaction,
        course_id: &CourseId,
        user_id: UserId,
        patch: CourseUserPatchRef<'a>,
    ) -> anyhow::Result<()> {
        queries::course::update_course_user()
            .params(
                txn.txn(),
                &UpdateCourseUserParams {
                    course_id: &**course_id,
                    user_id: *user_id,
                    purchased: patch.purchased.update().copied(),
                },
            )
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

fn decode_course_user(value: queries::course::CourseUser) -> CourseUser {
    CourseUser {
        course_id: value.course_id.into(),
        user_id: value.user_id.into(),
        purchased: value.purchased,
    }
}
