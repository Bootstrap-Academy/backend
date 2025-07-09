use academy_models::{
    auth::{AccessToken, AuthError},
    course::{CourseFilter, CourseId, CourseUserSummary},
};
use thiserror::Error;

pub trait CourseFeatureService: Send + Sync + 'static {
    /// Return summaries of all courses.
    fn list(
        &self,
        filter: CourseFilter,
    ) -> impl Future<Output = anyhow::Result<Vec<CourseUserSummary>>> + Send;

    /// Purchase a course for the authenticated user.
    ///
    /// Requires a verified email address.
    fn purchase(
        &self,
        token: &AccessToken,
        course_id: CourseId,
    ) -> impl Future<Output = Result<(), CoursePurchaseError>> + Send;
}

#[derive(Debug, Error)]
pub enum CoursePurchaseError {
    #[error("The course does not exist.")]
    CourseNotFound,
    #[error("The user has already purchased this course.")]
    AlreadyPurchased,
    #[error("The course is free.")]
    CourseIsFree,
    #[error("The user does not have enough coins.")]
    NotEnoughCoins,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
