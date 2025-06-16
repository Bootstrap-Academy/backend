use academy_core_course_contracts::CourseFeatureService;
use academy_data::course::CourseDataRepository;
use academy_di::Build;
use academy_models::course::{CourseFilter, CourseUserSummary};
use academy_utils::trace_instrument;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default, Build)]
pub struct CourseFeatureServiceImpl {
    course_data_repo: CourseDataRepository,
}

impl CourseFeatureService for CourseFeatureServiceImpl {
    #[trace_instrument(no_ret, skip(self))]
    async fn list(&self, filter: CourseFilter) -> anyhow::Result<Vec<CourseUserSummary>> {
        let mut courses = self
            .course_data_repo
            .values()
            .filter(|&course| {
                filter.search_term.as_ref().is_none_or(|search_term| {
                    course
                        .base
                        .title
                        .to_lowercase()
                        .contains(&search_term.to_lowercase())
                }) && filter.author.as_ref().is_none_or(|author| {
                    course
                        .base
                        .authors
                        .iter()
                        .any(|a| a.name.to_lowercase().contains(&author.to_lowercase()))
                }) && filter
                    .free
                    .is_none_or(|free| (course.base.price == 0) == free)
            })
            .cloned()
            .map(Into::into)
            .collect::<Vec<CourseUserSummary>>();

        if filter.search_term.is_some() {
            courses.sort_unstable_by_key(|c| c.base.title.len());
        } else {
            courses.sort_unstable_by(|a, b| a.base.title.cmp(&b.base.title));
        }

        Ok(courses)
    }
}
