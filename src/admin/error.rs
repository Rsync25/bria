use thiserror::Error;

use crate::error::*;

#[allow(clippy::large_enum_variant)]
#[derive(Error, Debug)]
pub enum AdminApiError {
    #[error("AdminApiError - TonicError: {0}")]
    TonicError(#[from] tonic::transport::Error),
    #[error("AdminApiError - SqlxError: {0}")]
    SqlxError(#[from] sqlx::Error),
}

impl From<BriaError> for AdminApiError {
    fn from(err: BriaError) -> Self {
        match err {
            BriaError::SqlxError(e) => AdminApiError::SqlxError(e),
        }
    }
}
