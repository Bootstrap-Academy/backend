use std::time::Duration;

use academy_utils::patch::Patch;
use chrono::{DateTime, Utc};

use crate::{SearchTerm, nutype_string, url::Url, user::UserId};

#[derive(Debug, Clone, PartialEq, Eq, Patch)]
pub struct CourseUser {
    #[no_patch]
    pub course_id: CourseId,
    #[no_patch]
    pub user_id: UserId,
    pub purchased: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseBase {
    pub id: CourseId,
    pub title: CourseTitle,
    pub description: CourseDescription,
    pub image_url: Option<Url>,
    pub authors: Vec<CourseAuthor>,
    pub price: u64,
    pub last_update: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Course {
    pub base: CourseBase,
    pub sections: Vec<CourseSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseUserSummary {
    pub base: CourseBase,
    pub sections: Vec<CourseSectionUserSummary>,
    pub completed: Option<bool>,
}

nutype_string!(CourseId);
nutype_string!(CourseTitle);
nutype_string!(CourseDescription);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseAuthor {
    pub name: CourseAuthorName,
    pub url: Option<Url>,
}

nutype_string!(CourseAuthorName);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseSection {
    pub id: CourseSectionId,
    pub title: CourseSectionTitle,
    pub lectures: Vec<CourseLecture>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseSectionUserSummary {
    pub title: CourseSectionTitle,
    pub lectures: Vec<CourseLectureUserSummary>,
    pub completed: Option<bool>,
}

nutype_string!(CourseSectionId);
nutype_string!(CourseSectionTitle);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseLecture {
    pub id: CourseLectureId,
    pub title: CourseLectureTitle,
    pub kind: CourseLectureKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseLectureUserSummary {
    pub title: CourseLectureTitle,
    pub duration: Duration,
    pub completed: Option<bool>,
}

nutype_string!(CourseLectureId);
nutype_string!(CourseLectureTitle);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CourseLectureKind {
    Youtube(CourseYoutubeLecture),
    Mp4(CourseMp4Lecture),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseYoutubeLecture {
    pub video_id: String,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseMp4Lecture {
    pub video_id: String,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CourseFilter {
    /// Search in `title`
    pub search_term: Option<SearchTerm>,
    /// Filter by author
    pub author: Option<SearchTerm>,
    /// Return only free (`true`) or unfree (`false`) courses
    pub free: Option<bool>,
}

impl From<Course> for CourseUserSummary {
    fn from(value: Course) -> Self {
        Self {
            base: value.base,
            sections: value.sections.into_iter().map(Into::into).collect(),
            completed: None,
        }
    }
}

impl From<CourseSection> for CourseSectionUserSummary {
    fn from(value: CourseSection) -> Self {
        Self {
            title: value.title,
            lectures: value.lectures.into_iter().map(Into::into).collect(),
            completed: None,
        }
    }
}

impl From<CourseLecture> for CourseLectureUserSummary {
    fn from(value: CourseLecture) -> Self {
        Self {
            title: value.title,
            duration: match value.kind {
                CourseLectureKind::Youtube(course_youtube_lecture) => {
                    course_youtube_lecture.duration
                }
                CourseLectureKind::Mp4(course_mp4_lecture) => course_mp4_lecture.duration,
            },
            completed: None,
        }
    }
}
