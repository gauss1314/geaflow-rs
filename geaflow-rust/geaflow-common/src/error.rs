use thiserror::Error;

pub type GeaFlowResult<T> = Result<T, GeaFlowError>;

#[derive(Debug, Error)]
pub enum GeaFlowError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal error: {0}")]
    Internal(String),
}
