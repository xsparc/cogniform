use core::fmt;

use cogniform_protocol::{JsonErrorCategory, ValidationError};

/// Stable classifications for invalid compilation-result values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilationValidationKind {
    /// The compilation-result schema version is unsupported.
    UnsupportedSchema,
    /// The decision collection exceeded its configured limit.
    DecisionLimitExceeded,
    /// The unresolved collection exceeded its configured limit.
    UnresolvedLimitExceeded,
    /// Aggregate scene text exceeded its configured limit.
    TextLimitExceeded,
    /// Logical decoded size exceeded its configured limit.
    DecodedSizeLimitExceeded,
    /// The optional normalized patch violated a core protocol invariant.
    InvalidPatch,
    /// Patch presence and unresolved entries do not describe one valid outcome.
    InvalidOutcome,
    /// The patch schema differs from the result schema.
    PatchSchemaMismatch,
    /// The patch base revision differs from the compilation scene revision.
    PatchRevisionMismatch,
    /// A decision's optional fields do not match its stable code.
    InvalidDecisionShape,
    /// An unresolved entry's optional fields do not match its stable code.
    InvalidUnresolvedShape,
    /// Decisions are not in canonical stable order.
    NonCanonicalDecisionOrder,
    /// The same canonical decision occurs more than once.
    DuplicateDecision,
    /// Unresolved entries are not in canonical stable order.
    NonCanonicalUnresolvedOrder,
    /// The same unresolved entry occurs more than once.
    DuplicateUnresolved,
}

/// A bounded compilation-result validation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompilationValidationError {
    kind: CompilationValidationKind,
    field: &'static str,
    protocol_error: Option<ValidationError>,
}

impl CompilationValidationError {
    pub(crate) const fn new(kind: CompilationValidationKind, field: &'static str) -> Self {
        Self {
            kind,
            field,
            protocol_error: None,
        }
    }

    pub(crate) const fn protocol(field: &'static str, error: ValidationError) -> Self {
        Self {
            kind: CompilationValidationKind::InvalidPatch,
            field,
            protocol_error: Some(error),
        }
    }

    /// Returns the stable validation classification.
    #[must_use]
    pub const fn kind(&self) -> CompilationValidationKind {
        self.kind
    }

    /// Returns the stable result-schema field path.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// Returns a nested core protocol failure when patch validation failed.
    #[must_use]
    pub const fn protocol_error(&self) -> Option<ValidationError> {
        self.protocol_error
    }
}

impl fmt::Display for CompilationValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(error) = self.protocol_error {
            write!(
                formatter,
                "invalid protocol value at {}: {error}",
                self.field
            )
        } else {
            write!(formatter, "{:?} at {}", self.kind, self.field)
        }
    }
}

impl std::error::Error for CompilationValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.protocol_error
            .as_ref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

/// Bounded canonical compilation-result codec failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilationCodecError {
    /// The encoded result exceeded the configured complete-message limit.
    EncodedSizeExceeded {
        /// Observed encoded bytes.
        actual: u64,
        /// Configured maximum encoded bytes.
        limit: u64,
    },
    /// JSON nesting exceeded the configured pre-decode limit.
    NestingLimitExceeded {
        /// Observed nesting depth.
        actual: u16,
        /// Configured maximum nesting depth.
        limit: u16,
    },
    /// JSON parsing failed without retaining input or an unbounded parser string.
    MalformedJson {
        /// Coarse parser classification.
        category: JsonErrorCategory,
        /// One-based parser line.
        line: usize,
        /// One-based parser column.
        column: usize,
    },
    /// The typed result violates a schema invariant or configured limit.
    InvalidResult(CompilationValidationError),
    /// The input is valid JSON but not the exact canonical representation.
    NonCanonicalResult,
    /// Canonical serialization failed for a validated result.
    SerializationFailed,
    /// A bounded output allocation could not be satisfied.
    AllocationFailed,
}

impl fmt::Display for CompilationCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncodedSizeExceeded { actual, limit } => {
                write!(
                    formatter,
                    "compilation result has {actual} bytes; limit is {limit}"
                )
            }
            Self::NestingLimitExceeded { actual, limit } => {
                write!(
                    formatter,
                    "JSON nesting depth is {actual}; limit is {limit}"
                )
            }
            Self::MalformedJson {
                category,
                line,
                column,
            } => write!(formatter, "{category:?} JSON error at {line}:{column}"),
            Self::InvalidResult(error) => error.fmt(formatter),
            Self::NonCanonicalResult => {
                formatter.write_str("compilation result is not canonical JSON followed by LF")
            }
            Self::SerializationFailed => {
                formatter.write_str("canonical compilation-result serialization failed")
            }
            Self::AllocationFailed => {
                formatter.write_str("bounded compilation-result allocation failed")
            }
        }
    }
}

impl std::error::Error for CompilationCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidResult(error) => Some(error),
            _ => None,
        }
    }
}
