use std::fmt::Write;

use academy_utils::Apply;
use aide::{
    OperationOutput,
    generate::in_context,
    openapi::{ReferenceOr, Response, Responses},
    transform::{TransformOperation, TransformResponse},
};
use axum::{Json, Router, http::StatusCode};
use schemars::{JsonSchema, Schema, json_schema};

use crate::errors::{ApiError, ApiErrorCode};

mod redoc;
mod swagger;

pub fn router() -> Router<()> {
    Router::new()
        .merge(swagger::router())
        .merge(redoc::router())
}

/// Extension trait for [`TransformOperation`]
pub trait TransformOperationExt {
    /// Add a [`Json`] response to the operation.
    ///
    /// Different responses with the same status code are automatically merged.
    fn add_response<R: JsonSchema>(
        self,
        code: StatusCode,
        description: impl Into<Option<&'static str>>,
    ) -> Self
    where
        Self: Sized,
    {
        self.add_response_with::<R>(code, description, |op| op)
    }

    /// Same as [`TransformOperationExt::add_response`], additionally accepting
    /// a transform function.
    fn add_response_with<R: JsonSchema>(
        self,
        code: StatusCode,
        description: impl Into<Option<&'static str>>,
        transform: impl FnOnce(TransformResponse<R>) -> TransformResponse<R>,
    ) -> Self;

    /// Add an [`ApiError`] response by its [`ApiErrorCode`].
    fn add_error<C: ApiErrorCode>(self) -> Self
    where
        Self: Sized,
    {
        self.add_response::<ApiError<C>>(
            C::STATUS_CODE,
            Some(C::DESCRIPTION).filter(|d| !d.is_empty()),
        )
    }
}

impl TransformOperationExt for TransformOperation<'_> {
    fn add_response_with<R: JsonSchema>(
        mut self,
        code: StatusCode,
        description: impl Into<Option<&'static str>>,
        transform: impl FnOnce(TransformResponse<R>) -> TransformResponse<R>,
    ) -> Self {
        let mut response =
            in_context(|ctx| Json::<R>::operation_response(ctx, &mut Default::default()).unwrap());
        if let Some(description) = description.into() {
            response.description = description.into();
        }
        let _ = transform(TransformResponse::new(&mut response));

        let operation = self.inner_mut();
        let responses = match &mut operation.responses {
            Some(responses) => responses,
            None => operation.responses.insert(Default::default()),
        };

        merge_into_responses(code, response, responses);

        self
    }
}

/// Merge the `src` [`Response`] into the `dst` [`Responses`]
fn merge_into_responses(code: StatusCode, src: Response, dst: &mut Responses) {
    let code = aide::openapi::StatusCode::Code(code.as_u16());

    let dst = match dst.responses.get_mut(&code) {
        Some(dst) => dst,
        None => {
            // no merging necessary if `dst` does not contain any response for `code` yet
            dst.responses.insert(code, ReferenceOr::Item(src));
            return;
        }
    };

    let ReferenceOr::Item(dst) = dst else {
        unimplemented!("cannot merge references yet")
    };

    // merge each media type individually
    for (media_type_name, src_media_type) in src.content {
        let dst_media_type = dst
            .content
            .entry(media_type_name)
            .or_insert_with(Default::default);
        match dst_media_type.schema.take() {
            // the media type already exists on `dst`, so merging is necessary
            Some(schema) => {
                // extract the schemas that already exist in `dst`
                let mut schemas = schema
                    .json_schema
                    .as_object()
                    .and_then(|schema| schema.get("anyOf"))
                    .and_then(|subschemas| subschemas.as_array())
                    .filter(|subschemas| !subschemas.is_empty())
                    .map(|subschemas| {
                        subschemas
                            .iter()
                            .map(|subschema| Schema::try_from(subschema.clone()).unwrap())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| {
                        let mut schema = schema.json_schema;
                        set_description(&mut schema, dst.description.clone());
                        vec![schema]
                    });

                // add the schema from `src` if `dst` does not already contain it
                schemas.extend(
                    src_media_type
                        .schema
                        .map(|v| {
                            v.json_schema
                                .with(|s| set_description(s, src.description.clone()))
                        })
                        .filter(|v| !schemas.contains(v)),
                );

                // build the description of the new (maybe combined) response
                let descriptions = schemas
                    .iter()
                    .map(|s| {
                        s.as_object()
                            .and_then(|s| s.get("title")?.as_str())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>();
                if descriptions.len() == 1 {
                    dst.description = descriptions[0].into();
                } else {
                    dst.description =
                        "There are multiple possible responses with this status code:".into();
                    for d in descriptions {
                        write!(&mut dst.description, "\n- {d}").unwrap();
                    }
                }

                dst_media_type.schema = Some(aide::openapi::SchemaObject {
                    json_schema: json_schema!({"anyOf": schemas}),
                    external_docs: None,
                    example: None,
                });
            }
            // the media type does not yet exist on `dst`, so no merging required
            None => dst_media_type.schema = src_media_type.schema,
        }
    }
}

/// Set the description of a [`Schema`].
fn set_description(schema: &mut Schema, description: String) {
    schema
        .ensure_object()
        .insert("title".into(), description.into());
}
