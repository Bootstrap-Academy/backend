#[cfg(feature = "mock")]
use academy_models::course::CourseUserPatch;
use academy_models::{
    course::{CourseId, CourseUser, CourseUserPatchRef},
    user::UserId,
};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait CourseRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return the given course user.
    fn get_course_user(
        &self,
        txn: &mut Txn,
        course_id: &CourseId,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<CourseUser>> + Send;

    /// Update the given course user.
    fn update_course_user<'a>(
        &self,
        txn: &mut Txn,
        course_id: &CourseId,
        user_id: UserId,
        patch: CourseUserPatchRef<'a>,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockCourseRepository<Txn> {
    pub fn with_get_course_user(
        mut self,
        course_id: CourseId,
        user_id: UserId,
        result: CourseUser,
    ) -> Self {
        self.expect_get_course_user()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(course_id),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_update_course_user(
        mut self,
        course_id: CourseId,
        user_id: UserId,
        patch: CourseUserPatch,
    ) -> Self {
        self.expect_update_course_user()
            .once()
            .withf(move |_, cid, uid, p| {
                *cid == course_id && *uid == user_id && *p == patch.as_ref()
            })
            .return_once(|_, _, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
