use academy_auth_contracts::MockAuthService;
use academy_core_coin_contracts::coin::MockCoinService;
use academy_data::course::CourseDataRepository;
use academy_demo::course::ALL_COURSES;
use academy_email_contracts::template::MockTemplateEmailService;
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, course::MockCourseRepository, user::MockUserRepository,
};

use crate::CourseFeatureServiceImpl;

mod list;
mod purchase;

type Sut = CourseFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockCoinService<MockTransaction>,
    MockTemplateEmailService,
    MockUserRepository<MockTransaction>,
    MockCourseRepository<MockTransaction>,
>;

fn course_data_repo() -> CourseDataRepository {
    ALL_COURSES.iter().copied().cloned().collect()
}
