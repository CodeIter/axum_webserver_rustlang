//! URL redirection endpoint.

use axum::{
    extract::{rejection::PathRejection, Path, RawQuery},
    response::Redirect,
};

use crate::{error::ApiError, services::redirect};

/// `GET /api/v1/redirect/{proto}/{domain}/{*path}?query=...`
///
/// Responds `307 Temporary Redirect` to `proto://domain/path?query`.
/// `{*path}` captures every remaining segment as one string.
pub async fn redirect(
    path: Result<Path<(String, String, String)>, PathRejection>,
    RawQuery(query): RawQuery,
) -> Result<Redirect, ApiError> {
    let Path((proto, domain, path)) = path?;
    let target = redirect::build_target(&proto, &domain, &path, query.as_deref())?;
    Ok(Redirect::temporary(&target))
}
