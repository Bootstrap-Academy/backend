use academy_data::course::CourseDataRepository;
use academy_demo::course::ALL_COURSES;

use crate::CourseFeatureServiceImpl;

mod list;

type Sut = CourseFeatureServiceImpl;

fn course_data_repo() -> CourseDataRepository {
    ALL_COURSES.iter().copied().cloned().collect()
}
