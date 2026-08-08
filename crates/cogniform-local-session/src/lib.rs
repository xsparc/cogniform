//! Bounded versioned control messages for one local Cogniform agent session.
//!
//! This crate assigns schema-owned meaning to CF039 control-frame bytes. It
//! validates canonical direction-specific messages and adapts them to caller-
//! owned frames, but opens no process, pipe, file, socket, or service loop and
//! performs no command, query, or observation work.

#![forbid(unsafe_code)]

mod codec;
mod error;
mod message;

pub use codec::{
    client_control_frame, decode_client_control_frame, decode_client_message,
    decode_server_control_frame, decode_server_message, encode_client_message,
    encode_server_message, server_control_frame,
};
pub use error::{LocalSessionError, LocalSessionValidationError, LocalSessionValidationKind};
pub use message::{
    ClientHello, LOCAL_SESSION_SCHEMA_VERSION, LocalSessionClientKind, LocalSessionClientMessage,
    LocalSessionLimits, LocalSessionServerKind, LocalSessionServerMessage, ObservationReference,
    PatchAdmission, PatchAdmissionStatus, PatchCompletion, QueryRequest, QueryResponse,
    RequestObservation, ServerHello, SessionClose, SessionClosed, SessionFailure,
    SessionFailureCode, SubmitPatch,
};

pub(crate) use message::SessionValidate;
