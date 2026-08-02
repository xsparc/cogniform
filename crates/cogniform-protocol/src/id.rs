use core::{fmt, num::NonZeroU64, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ValueError, ValueErrorKind};

macro_rules! opaque_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u128);

        impl $name {
            /// Constructs a non-zero opaque identifier.
            pub const fn new(value: u128) -> Result<Self, ValueError> {
                if value == 0 {
                    Err(ValueError::new(ValueErrorKind::ZeroIdentifier))
                } else {
                    Ok(Self(value))
                }
            }

            /// Returns the underlying 128-bit value.
            #[must_use]
            pub const fn get(self) -> u128 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{:032x}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = ValueError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                if value.len() != 32
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return Err(ValueError::new(ValueErrorKind::InvalidIdentifierEncoding));
                }

                let parsed = u128::from_str_radix(value, 16)
                    .map_err(|_| ValueError::new(ValueErrorKind::InvalidIdentifierEncoding))?;
                Self::new(parsed)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let encoded = String::deserialize(deserializer)?;
                encoded.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

opaque_id!(
    /// Opaque external entity identity that is independent of ECS handles.
    StableEntityId
);
opaque_id!(
    /// Identity shared by one patch request and its receipt.
    TransactionId
);
opaque_id!(
    /// Key used to return a recorded result without applying a patch twice.
    IdempotencyKey
);
opaque_id!(
    /// Identity of one requested or produced observation.
    ObservationId
);
opaque_id!(
    /// Identity of one semantic imagination request.
    ImaginationId
);

/// Monotonically increasing authoritative scene revision.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SceneRevision(u64);

impl SceneRevision {
    /// The initial empty-world revision.
    pub const INITIAL: Self = Self(0);

    /// Constructs a revision from its numeric value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the next revision or reports overflow.
    pub const fn checked_next(self) -> Result<Self, ValueError> {
        match self.0.checked_add(1) {
            Some(value) => Ok(Self(value)),
            None => Err(ValueError::new(ValueErrorKind::RevisionOverflow)),
        }
    }
}

/// Non-zero identity of a produced frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrameId(NonZeroU64);

impl FrameId {
    /// Constructs a frame identifier.
    pub const fn new(value: u64) -> Result<Self, ValueError> {
        match NonZeroU64::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(ValueError::new(ValueErrorKind::ZeroNumericIdentifier)),
        }
    }

    /// Returns the numeric frame identifier.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Non-zero version of a public message schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct SchemaVersion(u16);

impl SchemaVersion {
    /// Initial canonical JSON schema version.
    pub const V1: Self = Self(1);

    /// Constructs a non-zero schema version.
    pub const fn new(value: u16) -> Result<Self, ValueError> {
        if value == 0 {
            Err(ValueError::new(ValueErrorKind::ZeroSchemaVersion))
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the numeric schema version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for SchemaVersion {
    type Error = ValueError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SchemaVersion> for u16 {
    fn from(value: SchemaVersion) -> Self {
        value.get()
    }
}

/// Non-empty scene text that never contains a NUL character.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SceneText(String);

impl SceneText {
    /// Validates and owns a text value.
    pub fn new(value: impl Into<String>) -> Result<Self, ValueError> {
        Self::try_from(value.into())
    }

    /// Returns the UTF-8 text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the encoded UTF-8 byte count.
    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.0.len()
    }
}

impl TryFrom<String> for SceneText {
    type Error = ValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(ValueError::new(ValueErrorKind::EmptyText));
        }
        if value.contains('\0') {
            return Err(ValueError::new(ValueErrorKind::TextContainsNul));
        }
        Ok(Self(value))
    }
}

impl From<SceneText> for String {
    fn from(value: SceneText) -> Self {
        value.0
    }
}

macro_rules! float_value {
    ($(#[$meta:meta])* $name:ident, $validate:expr, $error:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(try_from = "f32", into = "f32")]
        pub struct $name(f32);

        impl $name {
            /// Validates and normalizes a floating-point value.
            pub fn new(value: f32) -> Result<Self, ValueError> {
                if !value.is_finite() {
                    return Err(ValueError::new(ValueErrorKind::NonFiniteNumber));
                }
                if !($validate)(value) {
                    return Err(ValueError::new($error));
                }
                Ok(Self(if value == 0.0 { 0.0 } else { value }))
            }

            /// Returns the validated value.
            #[must_use]
            pub const fn get(self) -> f32 {
                self.0
            }
        }

        impl TryFrom<f32> for $name {
            type Error = ValueError;

            fn try_from(value: f32) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for f32 {
            fn from(value: $name) -> Self {
                value.get()
            }
        }
    };
}

float_value!(
    /// Finite 32-bit floating-point value with negative zero normalized.
    FiniteF32,
    |_| true,
    ValueErrorKind::NonFiniteNumber
);
float_value!(
    /// Finite 32-bit floating-point value greater than zero.
    PositiveF32,
    |value: f32| value > 0.0,
    ValueErrorKind::NonPositiveNumber
);
float_value!(
    /// Finite 32-bit floating-point value greater than or equal to zero.
    NonNegativeF32,
    |value: f32| value >= 0.0,
    ValueErrorKind::NegativeNumber
);
float_value!(
    /// Finite 32-bit floating-point value in the inclusive unit interval.
    UnitF32,
    |value: f32| (0.0..=1.0).contains(&value),
    ValueErrorKind::OutsideUnitInterval
);
