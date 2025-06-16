use std::sync::Arc;

use academy_core_course_contracts::CourseFeatureService;
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::{
    docs::TransformOperationExt,
    errors::{internal_server_error, internal_server_error_docs},
    models::course::{ApiCourseFilter, ApiCourseUserSummary},
};

pub const TAG: &str = "Courses";

pub fn router(service: Arc<impl CourseFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route("/skills/courses", routing::get_with(list, list_docs))
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn list(
    service: State<Arc<impl CourseFeatureService>>,
    Query(filter): Query<ApiCourseFilter>,
) -> Response {
    match service.list(filter.into()).await {
        Ok(courses) => Json(
            courses
                .into_iter()
                .map(Into::into)
                .collect::<Vec<ApiCourseUserSummary>>(),
        )
        .into_response(),
        Err(err) => internal_server_error(err),
    }
}

fn list_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return summaries of all courses")
        .add_response::<Vec<ApiCourseUserSummary>>(StatusCode::OK, None)
        .with(internal_server_error_docs)
}
