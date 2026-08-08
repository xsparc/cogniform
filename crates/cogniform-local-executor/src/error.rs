use core::fmt;

/// Stable executor failure that never retains control or observation payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalExecutorError {
    /// Executor and service bounds are internally inconsistent.
    InvalidConfig {
        /// Stable non-sensitive configuration reason.
        reason: &'static str,
    },
    /// The supplied service already owns transient work the executor cannot correlate.
    ServiceNotQuiescent {
        /// Uncommitted mutating commands.
        commands: u32,
        /// Queued, active, or undelivered observations.
        observations: u32,
    },
    /// A validated response could not fit the negotiated output boundary.
    OutputRejected,
    /// Service and executor bookkeeping disagreed after a typed operation.
    StateInvariant {
        /// Stable non-sensitive invariant category.
        reason: &'static str,
    },
}

impl fmt::Display for LocalExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { reason } => {
                write!(formatter, "invalid local executor config: {reason}")
            }
            Self::ServiceNotQuiescent {
                commands,
                observations,
            } => write!(
                formatter,
                "local executor requires quiescence; commands={commands}, observations={observations}"
            ),
            Self::OutputRejected => {
                formatter.write_str("local executor output exceeded negotiated bounds")
            }
            Self::StateInvariant { reason } => {
                write!(formatter, "local executor state invariant failed: {reason}")
            }
        }
    }
}

impl std::error::Error for LocalExecutorError {}
