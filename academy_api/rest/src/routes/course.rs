use std::sync::Arc;

use academy_core_course_contracts::{CourseFeatureService, CoursePurchaseError};
use academy_models::course::CourseId;
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::{
        OkResponse,
        course::{ApiCourseFilter, ApiCourseUserSummary},
    },
    routes::coin::NotEnoughCoinsError,
};

pub const TAG: &str = "Courses";

pub fn router(service: Arc<impl CourseFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route("/skills/courses", routing::get_with(list, list_docs))
        .api_route(
            "/skills/course_access/{course_id}",
            routing::post_with(purchase, purchase_docs),
        )
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

#[derive(Deserialize, JsonSchema)]
struct PurchasePath {
    course_id: CourseId,
}

async fn purchase(
    service: State<Arc<impl CourseFeatureService>>,
    token: ApiToken,
    Path(PurchasePath { course_id }): Path<PurchasePath>,
) -> Response {
    match service.purchase(&token.0, course_id).await {
        Ok(()) => Json(OkResponse).into_response(),
        Err(CoursePurchaseError::CourseNotFound) => CourseNotFoundError.into_response(),
        Err(CoursePurchaseError::CourseIsFree) => CourseIsFreeError.into_response(),
        Err(CoursePurchaseError::AlreadyPurchased) => AlreadyPurchasedError.into_response(),
        Err(CoursePurchaseError::NotEnoughCoins) => NotEnoughCoinsError.into_response(),
        Err(CoursePurchaseError::Auth(err)) => auth_error(err),
        Err(CoursePurchaseError::Other(err)) => internal_server_error(err),
    }
}

fn purchase_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Purchase a course for the authenticated user")
        .add_response::<OkResponse>(StatusCode::OK, "The course has been purchased.")
        .add_error::<CourseNotFoundError>()
        .add_error::<CourseIsFreeError>()
        .add_error::<AlreadyPurchasedError>()
        .add_error::<NotEnoughCoinsError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

error_code! {
    /// The course does not exist.
    CourseNotFoundError(NOT_FOUND, "Course not found");
    /// The course is free.
    CourseIsFreeError(FORBIDDEN, "Course is free");
    /// The user has already purchased this course.
    AlreadyPurchasedError(FORBIDDEN, "Already purchased course");
}
