use academy_models::{
    SearchTerm,
    course::{
        CourseAuthor, CourseAuthorName, CourseDescription, CourseFilter, CourseId,
        CourseLectureTitle, CourseLectureUserSummary, CourseSectionTitle, CourseSectionUserSummary,
        CourseTitle, CourseUserSummary,
    },
    url::Url,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiCourseUserSummary {
    pub id: CourseId,
    pub title: CourseTitle,
    pub description: CourseDescription,
    pub category: Option<String>,
    pub language: Option<&'static str>,
    pub image: Option<Url>,
    pub authors: Vec<ApiCourseAuthor>,
    pub price: u64,
    pub learnings_goals: Vec<String>,
    pub requirements: Vec<String>,
    pub last_update: i64,
    pub sections: Vec<ApiCourseSectionUserSummary>,
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiCourseAuthor {
    pub name: CourseAuthorName,
    pub url: Option<Url>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiCourseSectionUserSummary {
    pub title: CourseSectionTitle,
    pub lectures: Vec<ApiCourseLectureUserSummary>,
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiCourseLectureUserSummary {
    pub title: CourseLectureTitle,
    pub duration: u64,
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct ApiCourseFilter {
    /// Search in `title`
    pub search_term: Option<SearchTerm>,
    /// Filter by author
    pub author: Option<SearchTerm>,
    /// Return only free (`true`) or unfree (`false`) courses
    pub free: Option<bool>,
}

impl From<CourseUserSummary> for ApiCourseUserSummary {
    fn from(value: CourseUserSummary) -> Self {
        Self {
            id: value.base.id,
            title: value.base.title,
            description: value.base.description,
            category: None,
            language: Some("de"),
            image: value.base.image_url,
            authors: value.base.authors.into_iter().map(Into::into).collect(),
            price: value.base.price,
            learnings_goals: vec![],
            requirements: vec![],
            last_update: value.base.last_update.timestamp(),
            sections: value.sections.into_iter().map(Into::into).collect(),
            completed: value.completed,
        }
    }
}

impl From<CourseAuthor> for ApiCourseAuthor {
    fn from(value: CourseAuthor) -> Self {
        Self {
            name: value.name,
            url: value.url,
        }
    }
}

impl From<CourseSectionUserSummary> for ApiCourseSectionUserSummary {
    fn from(value: CourseSectionUserSummary) -> Self {
        Self {
            title: value.title,
            lectures: value.lectures.into_iter().map(Into::into).collect(),
            completed: value.completed,
        }
    }
}

impl From<CourseLectureUserSummary> for ApiCourseLectureUserSummary {
    fn from(value: CourseLectureUserSummary) -> Self {
        Self {
            title: value.title,
            duration: value.duration.as_secs(),
            completed: value.completed,
        }
    }
}

impl From<ApiCourseFilter> for CourseFilter {
    fn from(value: ApiCourseFilter) -> Self {
        Self {
            search_term: value.search_term,
            author: value.author,
            free: value.free,
        }
    }
}
