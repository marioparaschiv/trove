use utoipa::{
    IntoResponses, Modify, OpenApi,
    openapi::{
        self,
        security::{Http, HttpAuthScheme, SecurityScheme},
    },
};

use super::{error::ErrorBody, runtime::RuntimeFile};
use crate::model_setup;

#[derive(OpenApi)]
#[openapi(
    info(title = "Trove", description = "Local search over an Obsidian vault"),
    components(schemas(RuntimeFile, model_setup::Event, model_setup::Source)),
    security(("token" = [])),
    modifiers(&Adjust),
)]
pub(super) struct ApiDoc;

struct Adjust;

impl Modify for Adjust {
    fn modify(&self, api: &mut openapi::OpenApi) {
        // utoipa copies the license from Cargo metadata, which this workspace doesn't set.
        api.info.license = None;
        api.components
            .get_or_insert_with(Default::default)
            .add_security_scheme(
                "token",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
    }
}

/// Responses the guard gives before any handler runs. Every path lists these.
#[derive(IntoResponses)]
#[expect(dead_code, reason = "only describes responses in the OpenAPI spec")]
pub(super) enum Rejected {
    /// The `Authorization` header is missing or has the wrong token.
    #[response(status = 401)]
    Unauthorized(ErrorBody),
    /// The `Host` header is not this service's localhost address.
    #[response(status = 403)]
    Forbidden(ErrorBody),
}

/// The OpenAPI spec of the HTTP API, which the TypeScript client is generated from.
pub fn openapi() -> openapi::OpenApi {
    super::router().into_openapi()
}
