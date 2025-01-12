use std::fmt::Write;

use aide::{
    generate::in_context,
    openapi::{ReferenceOr, Response, Responses},
    transform::{TransformOperation, TransformResponse},
    OperationOutput,
};
use axum::{http::StatusCode, Json, Router};
use schemars::{
    schema::{Metadata, Schema, SchemaObject, SubschemaValidation},
    JsonSchema,
};

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
                // convert the aide SchemaObject into a schemars SchemaObject
                let schema = schema.json_schema.into_object();

                // extract the schemas that already exist in `dst`
                let mut schemas = if schema
                    .subschemas
                    .as_ref()
                    .and_then(|s| s.any_of.as_ref())
                    .is_some_and(|s| !s.is_empty())
                {
                    // `dst` already contains multiple schemas
                    schema.subschemas.unwrap().any_of.unwrap()
                } else {
                    // `dst` is a single schema
                    vec![schema_with_description(schema, dst.description.clone())]
                };

                // add the schema from `src` if `dst` does not already contain it
                schemas.extend(
                    src_media_type
                        .schema
                        .map(|v| {
                            schema_with_description(
                                v.json_schema.into_object(),
                                src.description.clone(),
                            )
                        })
                        .filter(|v| !schemas.contains(v)),
                );

                // build the description of the new (maybe combined) response
                let descriptions = schemas
                    .iter()
                    .map(|s| {
                        match s {
                            Schema::Bool(_) => None,
                            Schema::Object(obj) => obj.metadata.as_ref(),
                        }
                        .and_then(|m| m.title.as_deref())
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
                    json_schema: SchemaObject {
                        subschemas: Some(
                            SubschemaValidation {
                                any_of: Some(schemas),
                                ..Default::default()
                            }
                            .into(),
                        ),
                        ..Default::default()
                    }
                    .into(),
                    external_docs: None,
                    example: None,
                });
            }
            // the media type does not yet exist on `dst`, so no merging required
            None => dst_media_type.schema = src_media_type.schema,
        }
    }
}

/// Convert a [`SchemaObject`] into a [`Schema`] with the given `description`
fn schema_with_description(schema_object: SchemaObject, description: String) -> Schema {
    SchemaObject {
        metadata: Some(
            Metadata {
                title: Some(description),
                ..Default::default()
            }
            .into(),
        ),
        ..schema_object
    }
    .into()
}
