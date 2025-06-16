use academy_models::course::{CourseFilter, CourseUserSummary};

pub trait CourseFeatureService: Send + Sync + 'static {
    /// Return summaries of all courses.
    fn list(
        &self,
        filter: CourseFilter,
    ) -> impl Future<Output = anyhow::Result<Vec<CourseUserSummary>>> + Send;
}
