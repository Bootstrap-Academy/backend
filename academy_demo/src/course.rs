use std::sync::LazyLock;

use academy_models::course::{Course, CourseAuthor, CourseBase};
use chrono::{TimeZone, Utc};

pub static ALL_COURSES: LazyLock<Vec<&Course>> = LazyLock::new(|| vec![&COURSE1, &COURSE2]);

pub static COURSE_AUTHOR1: LazyLock<CourseAuthor> = LazyLock::new(|| CourseAuthor {
    name: "Course Author 1".into(),
    url: None,
});

pub static COURSE_AUTHOR2: LazyLock<CourseAuthor> = LazyLock::new(|| CourseAuthor {
    name: "Course Author 2".into(),
    url: None,
});

pub static COURSE1: LazyLock<Course> = LazyLock::new(|| Course {
    base: CourseBase {
        id: "c1".into(),
        title: "Course 1".into(),
        description: "desc1".into(),
        image_url: None,
        authors: vec![COURSE_AUTHOR1.clone()],
        price: 0,
        last_update: Utc.with_ymd_and_hms(2025, 7, 10, 16, 0, 0).unwrap(),
    },
    sections: vec![],
});

pub static COURSE2: LazyLock<Course> = LazyLock::new(|| Course {
    base: CourseBase {
        id: "c2".into(),
        title: "Course 2".into(),
        description: "desc2".into(),
        image_url: None,
        authors: vec![COURSE_AUTHOR1.clone(), COURSE_AUTHOR2.clone()],
        price: 42,
        last_update: Utc.with_ymd_and_hms(2025, 7, 10, 18, 0, 0).unwrap(),
    },
    sections: vec![],
});
