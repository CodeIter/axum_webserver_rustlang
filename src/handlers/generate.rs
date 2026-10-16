//! Random number and password endpoints.

use axum::{
    extract::{
        rejection::{PathRejection, QueryRejection},
        Path, Query,
    },
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiError,
    services::{number, password},
};

#[derive(Serialize)]
pub struct NumberResponse {
    min: i64,
    max: i64,
    quantity: usize,
    numbers: Vec<i64>,
}

/// `GET /api/v1/number/{min}/{max}/{quantity}`
pub async fn number(
    path: Result<Path<(i64, i64, usize)>, PathRejection>,
) -> Result<Json<NumberResponse>, ApiError> {
    let Path((min, max, quantity)) = path?;
    let numbers = number::generate(min, max, quantity)?;
    Ok(Json(NumberResponse { min, max, quantity, numbers }))
}

#[derive(Deserialize)]
pub struct PasswordQuery {
    /// Optional custom character set.
    chars: Option<String>,
}

#[derive(Serialize)]
pub struct PasswordResponse {
    length: usize,
    quantity: usize,
    passwords: Vec<String>,
}

/// `GET /api/v1/password/{length}/{quantity}?chars=<optional>`
pub async fn password(
    path: Result<Path<(usize, usize)>, PathRejection>,
    query: Result<Query<PasswordQuery>, QueryRejection>,
) -> Result<Json<PasswordResponse>, ApiError> {
    let Path((length, quantity)) = path?;
    let Query(PasswordQuery { chars }) = query?;
    let passwords = password::generate(length, quantity, chars.as_deref())?;
    Ok(Json(PasswordResponse { length, quantity, passwords }))
}
