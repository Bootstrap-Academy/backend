use std::{collections::HashMap, fs::DirEntry, ops::Deref, path::Path, sync::Arc, time::Duration};

use academy_models::{
    course::{
        Course, CourseAuthor, CourseAuthorName, CourseDescription, CourseId, CourseLecture,
        CourseLectureId, CourseLectureKind, CourseLectureTitle, CourseMp4Lecture, CourseSection,
        CourseSectionId, CourseSectionTitle, CourseTitle, CourseYoutubeLecture,
    },
    url::Url,
};
use anyhow::{Context, anyhow};
use chrono::DateTime;
use serde::Deserialize;
use tracing::{debug, info};

#[derive(Debug, Clone, Default)]
pub struct CourseDataRepository(Arc<HashMap<CourseId, Course>>);

impl Deref for CourseDataRepository {
    type Target = HashMap<CourseId, Course>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl CourseDataRepository {
    pub fn load(course_dir: &Path) -> anyhow::Result<Self> {
        info!("Loading courses from {}", course_dir.display());
        let courses = std::fs::read_dir(course_dir)
            .with_context(|| anyhow!("Failed to read directory at {}", course_dir.display()))?
            .map(|entry| load_course(entry?))
            .flat_map(|result| result.transpose())
            .collect::<anyhow::Result<_>>()?;

        Ok(Self(Arc::new(courses)))
    }
}

fn load_course(entry: DirEntry) -> anyhow::Result<Option<(CourseId, Course)>> {
    if !entry.file_type()?.is_file() {
        debug!(
            "Skipping {} because it is not a regular file",
            entry.path().display()
        );
        return Ok(None);
    }

    let file_name = entry.file_name();
    let file_name = file_name.to_str().ok_or_else(|| {
        anyhow!(
            "Name of file at {} is not valid unicode",
            entry.path().display()
        )
    })?;

    let Some(id) = file_name.strip_suffix(".yml") else {
        debug!(
            "Skipping {} because it's not a .yml file",
            entry.path().display()
        );
        return Ok(None);
    };

    let content = std::fs::read_to_string(entry.path())
        .with_context(|| anyhow!("Failed to read file at {}", entry.path().display()))?;

    let id = CourseId::new(id);
    let course = serde_yaml::from_str::<RawCourse>(&content)
        .with_context(|| anyhow!("Failed to deserialize file at {}", entry.path().display()))?;

    let course = Course {
        id: id.clone(),
        title: course.title,
        description: course.description,
        image_url: course.image,
        authors: course.authors.into_iter().map(Into::into).collect(),
        price: course.price,
        last_update: DateTime::from_timestamp(course.last_update, 0)
            .ok_or_else(|| anyhow!("Invalid timestamp: {}", course.last_update))?,
        sections: course.sections.into_iter().map(Into::into).collect(),
    };

    Ok(Some((id, course)))
}

#[derive(Debug, Deserialize)]
struct RawCourse {
    title: CourseTitle,
    description: CourseDescription,
    image: Option<Url>,
    authors: Vec<RawCourseAuthor>,
    price: u64,
    last_update: i64,
    sections: Vec<RawCourseSection>,
}

#[derive(Debug, Deserialize)]
struct RawCourseAuthor {
    name: CourseAuthorName,
    url: Option<Url>,
}

#[derive(Debug, Deserialize)]
struct RawCourseSection {
    id: CourseSectionId,
    title: CourseSectionTitle,
    lectures: Vec<RawCourseLecture>,
}

#[derive(Debug, Deserialize)]
struct RawCourseLecture {
    id: CourseLectureId,
    title: CourseLectureTitle,
    #[serde(flatten)]
    kind: RawCourseLectureKind,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RawCourseLectureKind {
    Youtube(RawCourseYoutubeLecture),
    Mp4(RawCourseMp4Lecture),
}

#[derive(Debug, Deserialize)]
struct RawCourseYoutubeLecture {
    video_id: String,
    duration: u64,
}

#[derive(Debug, Deserialize)]
struct RawCourseMp4Lecture {
    duration: u64,
}

impl From<RawCourseAuthor> for CourseAuthor {
    fn from(value: RawCourseAuthor) -> Self {
        Self {
            name: value.name,
            url: value.url,
        }
    }
}

impl From<RawCourseSection> for CourseSection {
    fn from(value: RawCourseSection) -> Self {
        Self {
            id: value.id,
            title: value.title,
            lectures: value.lectures.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<RawCourseLecture> for CourseLecture {
    fn from(value: RawCourseLecture) -> Self {
        Self {
            id: value.id.clone(),
            title: value.title,
            kind: match value.kind {
                RawCourseLectureKind::Youtube(kind) => {
                    CourseLectureKind::Youtube(CourseYoutubeLecture {
                        video_id: kind.video_id,
                        duration: Duration::from_secs(kind.duration),
                    })
                }
                RawCourseLectureKind::Mp4(kind) => CourseLectureKind::Mp4(CourseMp4Lecture {
                    video_id: value.id.into_inner(),
                    duration: Duration::from_secs(kind.duration),
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load() {
        CourseDataRepository::load(concat!(env!("CARGO_MANIFEST_DIR"), "/courses").as_ref())
            .unwrap();
    }
}
