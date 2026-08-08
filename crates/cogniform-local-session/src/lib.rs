//! Bounded versioned control messages for one local Cogniform agent session.
//!
//! This crate assigns schema-owned meaning to CF039 control-frame bytes. It
//! validates canonical direction-specific messages and adapts them to caller-
//! owned frames, but opens no process, pipe, file, socket, or service loop and
//! performs no compiler, command, query, or observation work.

#![forbid(unsafe_code)]

mod codec;
mod error;
mod message;

pub use codec::{
    client_control_frame, client_control_frame_with_limits, decode_client_control_frame,
    decode_client_control_frame_with_limits, decode_client_message,
    decode_client_message_with_limits, decode_server_control_frame,
    decode_server_control_frame_with_limits, decode_server_message,
    decode_server_message_with_limits, encode_client_message, encode_client_message_with_limits,
    encode_server_message, encode_server_message_with_limits, server_control_frame,
    server_control_frame_with_limits,
};
pub use error::{LocalSessionError, LocalSessionValidationError, LocalSessionValidationKind};
pub use message::{
    ClientHello, ImaginationAdmission, ImaginationAdmissionStatus, ImaginationCompletion,
    LOCAL_SESSION_SCHEMA_VERSION, LOCAL_SESSION_SCHEMA_VERSION_V2, LocalSessionClientKind,
    LocalSessionClientMessage, LocalSessionLimits, LocalSessionServerKind,
    LocalSessionServerMessage, ObservationReference, PatchAdmission, PatchAdmissionStatus,
    PatchCompletion, QueryRequest, QueryResponse, RequestObservation, ServerHello, SessionClose,
    SessionClosed, SessionFailure, SessionFailureCode, SubmitImagination, SubmitPatch,
    compilation_limits_fit, intersect_compilation_limits, validate_compilation_limits,
};

pub(crate) use message::SessionValidate;
