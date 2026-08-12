use academy_core_course_contracts::CourseFeatureService;
use academy_demo::course::{COURSE1, COURSE2};
use academy_models::course::{CourseFilter, CourseUserSummary};

use crate::{
    CourseFeatureServiceImpl,
    tests::{Sut, course_data_repo},
};

#[tokio::test]
async fn ok() {
    // Arrange
    let c1_summary = CourseUserSummary {
        base: COURSE1.base.clone(),
        sections: vec![],
        completed: None,
    };
    let c2_summary = CourseUserSummary {
        base: COURSE2.base.clone(),
        sections: vec![],
        completed: None,
    };

    let sut = CourseFeatureServiceImpl {
        course_data_repo: course_data_repo(),
        ..Sut::default()
    };

    for (filter, expected) in [
        (
            CourseFilter::default(),
            vec![c1_summary.clone(), c2_summary.clone()],
        ),
        (
            CourseFilter {
                search_term: Some("course 1".try_into().unwrap()),
                ..Default::default()
            },
            vec![c1_summary.clone()],
        ),
        (
            CourseFilter {
                author: Some("author 2".try_into().unwrap()),
                ..Default::default()
            },
            vec![c2_summary.clone()],
        ),
        (
            CourseFilter {
                free: Some(true),
                ..Default::default()
            },
            vec![c1_summary.clone()],
        ),
    ] {
        // Act
        let result = sut.list(filter).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }
}
