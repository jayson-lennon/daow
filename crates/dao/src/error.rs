use std::fmt;

/// Error type for all dao operations.
///
/// Wraps rusqlite errors, provides custom error variants for user-defined
/// type conversions, and reports column-level mismatches.
pub enum Error {
    /// A column with the given name was not found in the query result.
    ColumnNotFound { name: String },

    /// A column value could not be converted to the expected Rust type.
    TypeMismatch {
        column: String,
        expected: &'static str,
        got: &'static str,
    },

    /// An error from the underlying rusqlite operation.
    Database(rusqlite::Error),

    /// A user-defined error with a message.
    Custom(String),

    /// A user-defined error with a boxed error chain.
    CustomBoxed(Box<dyn std::error::Error + Send + Sync>),

    /// A pooled connection could not be acquired within the configured timeout.
    AcquireTimeout,
}

impl Error {
    /// Create a custom error from a message string.
    pub fn custom(msg: impl Into<String>) -> Self {
        Error::Custom(msg.into())
    }

    /// Create a custom error from a boxed error.
    pub fn custom_boxed(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Error::CustomBoxed(err)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Error::Database(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ColumnNotFound { name } => write!(f, "column not found: {name}"),
            Error::TypeMismatch {
                column,
                expected,
                got,
            } => write!(
                f,
                "type mismatch for column '{column}': expected {expected}, got {got}"
            ),
            Error::Database(err) => write!(f, "database error: {err}"),
            Error::Custom(msg) => write!(f, "{msg}"),
            Error::CustomBoxed(err) => write!(f, "{err}"),
            Error::AcquireTimeout => write!(f, "timed out waiting for a pooled connection"),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ColumnNotFound { name } => f
                .debug_struct("ColumnNotFound")
                .field("name", name)
                .finish(),
            Error::TypeMismatch {
                column,
                expected,
                got,
            } => f
                .debug_struct("TypeMismatch")
                .field("column", column)
                .field("expected", expected)
                .field("got", got)
                .finish(),
            Error::Database(err) => f.debug_tuple("Database").field(err).finish(),
            Error::Custom(msg) => f.debug_tuple("Custom").field(msg).finish(),
            Error::CustomBoxed(err) => f.debug_tuple("CustomBoxed").field(err).finish(),
            Error::AcquireTimeout => f.debug_tuple("AcquireTimeout").finish(),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Database(err) => Some(err),
            Error::CustomBoxed(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

/// Convenience type alias for results in dao operations.
pub type Result<T> = std::result::Result<T, Error>;
