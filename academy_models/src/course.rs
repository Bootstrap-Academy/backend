use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::{nutype_string, url::Url};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Course {
    pub id: CourseId,
    pub title: CourseTitle,
    pub description: CourseDescription,
    pub image_url: Option<Url>,
    pub authors: Vec<CourseAuthor>,
    pub price: u64,
    pub last_update: DateTime<Utc>,
    pub sections: Vec<CourseSection>,
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

nutype_string!(CourseSectionId);
nutype_string!(CourseSectionTitle);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseLecture {
    pub id: CourseLectureId,
    pub title: CourseLectureTitle,
    pub kind: CourseLectureKind,
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
