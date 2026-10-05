use core::{fmt, num::NonZeroU64};
use std::io::{self, Write};

use cogniform_protocol::{ImageDimensions, ObservationKind, RuntimeLimits, StableEntityId};

use crate::ObservationPayload;

/// Media type emitted by the diagnostic PNG projection.
pub const DIAGNOSTIC_PNG_MIME_TYPE: &str = "image/png";

/// Independent byte limit for one diagnostic PNG projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticPngLimits {
    /// Maximum complete encoded PNG bytes.
    pub max_png_bytes: NonZeroU64,
}

impl DiagnosticPngLimits {
    /// Constructs an explicit non-zero PNG byte limit.
    #[must_use]
    pub const fn new(max_png_bytes: NonZeroU64) -> Self {
        Self { max_png_bytes }
    }
}

impl Default for DiagnosticPngLimits {
    fn default() -> Self {
        Self {
            max_png_bytes: NonZeroU64::new(1_048_576).expect("constant is non-zero"),
        }
    }
}

/// Borrowed image values accepted by the diagnostic PNG projection.
#[derive(Debug, Clone, Copy)]
pub enum DiagnosticPngSource<'a> {
    /// Linear RGBA8 pixels.
    Color(&'a [[u8; 4]]),
    /// Normalized depth values.
    Depth(&'a [f32]),
    /// Optional world-space unit normals.
    Normal(&'a [Option<[f32; 3]>]),
    /// Optional stable entity identities.
    EntityId(&'a [Option<StableEntityId>]),
}

impl DiagnosticPngSource<'_> {
    fn item_count(self) -> u64 {
        let count = match self {
            Self::Color(values) => values.len(),
            Self::Depth(values) => values.len(),
            Self::Normal(values) => values.len(),
            Self::EntityId(values) => values.len(),
        };
        u64::try_from(count).unwrap_or(u64::MAX)
    }
}

impl<'a> TryFrom<&'a ObservationPayload> for DiagnosticPngSource<'a> {
    type Error = DiagnosticPngError;

    fn try_from(payload: &'a ObservationPayload) -> Result<Self, Self::Error> {
        match payload {
            ObservationPayload::Color(values) => Ok(Self::Color(values)),
            ObservationPayload::Depth(values) => Ok(Self::Depth(values)),
            ObservationPayload::Normal(values) => Ok(Self::Normal(values)),
            ObservationPayload::EntityId(values) => Ok(Self::EntityId(values)),
            ObservationPayload::Visibility(_) => Err(DiagnosticPngError::UnsupportedKind {
                kind: ObservationKind::Visibility,
            }),
        }
    }
}

/// Fail-closed error from diagnostic PNG validation or encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticPngError {
    /// The source kind has no image projection.
    UnsupportedKind {
        /// Unsupported observation kind.
        kind: ObservationKind,
    },
    /// Dimensions exceed the supplied observation runtime limits.
    DimensionLimitExceeded,
    /// The source item count disagrees with the dimensions.
    ItemCountMismatch {
        /// Required pixel count.
        expected: u64,
        /// Observed source count.
        actual: u64,
    },
    /// Checked size arithmetic or platform conversion overflowed.
    SizeOverflow,
    /// Memory reservation failed after bounds passed.
    AllocationFailed,
    /// The complete encoded PNG exceeded its explicit byte limit.
    PngLimitExceeded {
        /// Configured maximum bytes.
        limit: u64,
    },
    /// The pinned PNG encoder rejected otherwise bounded input.
    EncodingFailed,
}

impl fmt::Display for DiagnosticPngError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedKind { kind } => {
                write!(formatter, "observation kind {kind:?} has no diagnostic PNG")
            }
            Self::DimensionLimitExceeded => {
                formatter.write_str("diagnostic PNG dimensions exceed runtime limits")
            }
            Self::ItemCountMismatch { expected, actual } => write!(
                formatter,
                "diagnostic PNG source count {actual} does not match required count {expected}"
            ),
            Self::SizeOverflow => formatter.write_str("diagnostic PNG size overflow"),
            Self::AllocationFailed => formatter.write_str("diagnostic PNG allocation failed"),
            Self::PngLimitExceeded { limit } => {
                write!(formatter, "diagnostic PNG exceeds byte limit {limit}")
            }
            Self::EncodingFailed => formatter.write_str("diagnostic PNG encoding failed"),
        }
    }
}

impl std::error::Error for DiagnosticPngError {}

/// Encodes one bounded diagnostic-only PNG from borrowed observation values.
///
/// The output is a lossy presentation derivative. Exact numeric values and
/// stable identities remain authoritative only in the observation payload.
pub fn encode_diagnostic_png(
    dimensions: ImageDimensions,
    source: DiagnosticPngSource<'_>,
    runtime_limits: &RuntimeLimits,
    png_limits: DiagnosticPngLimits,
) -> Result<Vec<u8>, DiagnosticPngError> {
    let pixel_count = dimensions.pixel_count();
    if dimensions.width > runtime_limits.max_observation_width
        || dimensions.height > runtime_limits.max_observation_height
        || pixel_count > runtime_limits.max_observation_pixels.get()
    {
        return Err(DiagnosticPngError::DimensionLimitExceeded);
    }
    let actual = source.item_count();
    if actual != pixel_count {
        return Err(DiagnosticPngError::ItemCountMismatch {
            expected: pixel_count,
            actual,
        });
    }

    let (color_type, linear_gamma, pixels) = visualize(source, pixel_count)?;
    encode_png(
        dimensions.width.get(),
        dimensions.height.get(),
        color_type,
        &pixels,
        linear_gamma,
        png_limits,
    )
}

/// Returns the deterministic display color used for one stable entity.
///
/// Display colors are cosmetic and may collide. The stable entity identity in
/// the canonical observation payload remains authoritative.
#[must_use]
pub fn diagnostic_identity_color(entity_id: StableEntityId) -> [u8; 4] {
    let [
        b0,
        b1,
        b2,
        b3,
        b4,
        b5,
        b6,
        b7,
        b8,
        b9,
        b10,
        b11,
        b12,
        b13,
        b14,
        b15,
    ] = entity_id.get().to_le_bytes();
    let low = u64::from_le_bytes([b0, b1, b2, b3, b4, b5, b6, b7]);
    let high = u64::from_le_bytes([b8, b9, b10, b11, b12, b13, b14, b15]);
    let mut mixed = low ^ high.rotate_left(29);
    mixed = mixed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^= mixed >> 31;
    let bytes = mixed.to_le_bytes();
    [
        visible_channel(bytes[0]),
        visible_channel(bytes[1]),
        visible_channel(bytes[2]),
        255,
    ]
}

fn visualize(
    source: DiagnosticPngSource<'_>,
    pixel_count: u64,
) -> Result<(png::ColorType, bool, Vec<u8>), DiagnosticPngError> {
    let channels = if matches!(source, DiagnosticPngSource::Depth(_)) {
        1_u64
    } else {
        4_u64
    };
    let byte_count = pixel_count
        .checked_mul(channels)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(DiagnosticPngError::SizeOverflow)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(byte_count)
        .map_err(|_| DiagnosticPngError::AllocationFailed)?;

    match source {
        DiagnosticPngSource::Color(values) => {
            for pixel in values {
                pixels.extend_from_slice(pixel);
            }
            Ok((png::ColorType::Rgba, true, pixels))
        }
        DiagnosticPngSource::Depth(values) => {
            pixels.extend(values.iter().map(|depth| unit_to_byte(1.0 - depth)));
            Ok((png::ColorType::Grayscale, false, pixels))
        }
        DiagnosticPngSource::Normal(values) => {
            for value in values {
                match value {
                    None => pixels.extend_from_slice(&[0, 0, 0, 0]),
                    Some(normal) => pixels.extend_from_slice(&[
                        signed_unit_to_byte(normal[0]),
                        signed_unit_to_byte(normal[1]),
                        signed_unit_to_byte(normal[2]),
                        255,
                    ]),
                }
            }
            Ok((png::ColorType::Rgba, false, pixels))
        }
        DiagnosticPngSource::EntityId(values) => {
            for value in values {
                pixels.extend_from_slice(&value.map_or([0, 0, 0, 0], diagnostic_identity_color));
            }
            Ok((png::ColorType::Rgba, false, pixels))
        }
    }
}

fn encode_png(
    width: u32,
    height: u32,
    color_type: png::ColorType,
    pixels: &[u8],
    linear_gamma: bool,
    limits: DiagnosticPngLimits,
) -> Result<Vec<u8>, DiagnosticPngError> {
    let maximum = usize::try_from(limits.max_png_bytes.get())
        .map_err(|_| DiagnosticPngError::SizeOverflow)?;
    let mut output = BoundedWriter::new(maximum);
    let encoded = {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(color_type);
        encoder.set_depth(png::BitDepth::Eight);
        if linear_gamma {
            encoder.set_source_gamma(png::ScaledFloat::new(1.0));
        }
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(pixels))
    };
    if encoded.is_err() {
        return Err(match output.failure {
            Some(WriterFailure::Limit) => DiagnosticPngError::PngLimitExceeded {
                limit: limits.max_png_bytes.get(),
            },
            Some(WriterFailure::Allocation) => DiagnosticPngError::AllocationFailed,
            Some(WriterFailure::Overflow) => DiagnosticPngError::SizeOverflow,
            None => DiagnosticPngError::EncodingFailed,
        });
    }
    Ok(output.bytes)
}

fn visible_channel(value: u8) -> u8 {
    let scaled = (u16::from(value) * 191) / 255;
    64 + u8::try_from(scaled).expect("scaled color channel is at most 191")
}

fn signed_unit_to_byte(value: f32) -> u8 {
    unit_to_byte(value.mul_add(0.5, 0.5))
}

// The clamp and scale prove the rounded value is finite and within u8.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn unit_to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriterFailure {
    Limit,
    Allocation,
    Overflow,
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
    failure: Option<WriterFailure>,
}

impl BoundedWriter {
    fn new(maximum: usize) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
            failure: None,
        }
    }

    fn fail(&mut self, failure: WriterFailure) -> io::Error {
        self.failure = Some(failure);
        io::Error::other("diagnostic PNG output unavailable")
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(required) = self.bytes.len().checked_add(buffer.len()) else {
            return Err(self.fail(WriterFailure::Overflow));
        };
        if required > self.maximum {
            return Err(self.fail(WriterFailure::Limit));
        }
        if self.bytes.try_reserve_exact(buffer.len()).is_err() {
            return Err(self.fail(WriterFailure::Allocation));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU64;
    use std::io::Cursor;

    use cogniform_protocol::{ImageDimensions, RuntimeLimits, StableEntityId};

    use super::{
        DiagnosticPngError, DiagnosticPngLimits, DiagnosticPngSource, diagnostic_identity_color,
        encode_diagnostic_png,
    };

    fn dimensions(width: u32, height: u32) -> ImageDimensions {
        ImageDimensions {
            width: core::num::NonZeroU32::new(width).unwrap(),
            height: core::num::NonZeroU32::new(height).unwrap(),
        }
    }

    fn decode(bytes: Vec<u8>) -> (png::OutputInfo, Vec<u8>, Option<png::ScaledFloat>) {
        let decoder = png::Decoder::new(Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        let gamma = reader.info().gamma();
        (info, pixels, gamma)
    }

    #[test]
    fn color_projection_preserves_rgba_and_linear_gamma() {
        let source = [[1, 2, 3, 4], [5, 6, 7, 8]];
        let encoded = encode_diagnostic_png(
            dimensions(2, 1),
            DiagnosticPngSource::Color(&source),
            &RuntimeLimits::default(),
            DiagnosticPngLimits::default(),
        )
        .unwrap();
        let (info, pixels, gamma) = decode(encoded);
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        assert_eq!(pixels, source.concat());
        assert_eq!(gamma, Some(png::ScaledFloat::new(1.0)));
    }

    #[test]
    fn depth_and_normal_projections_preserve_existing_visualization_rules() {
        let depth = encode_diagnostic_png(
            dimensions(3, 1),
            DiagnosticPngSource::Depth(&[0.0, 0.5, 1.0]),
            &RuntimeLimits::default(),
            DiagnosticPngLimits::default(),
        )
        .unwrap();
        let (info, pixels, gamma) = decode(depth);
        assert_eq!(info.color_type, png::ColorType::Grayscale);
        assert_eq!(pixels, [255, 128, 0]);
        assert_eq!(gamma, None);

        let normals = encode_diagnostic_png(
            dimensions(2, 1),
            DiagnosticPngSource::Normal(&[None, Some([-1.0, 0.0, 1.0])]),
            &RuntimeLimits::default(),
            DiagnosticPngLimits::default(),
        )
        .unwrap();
        let (info, pixels, gamma) = decode(normals);
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(pixels, [0, 0, 0, 0, 0, 128, 255, 255]);
        assert_eq!(gamma, None);
    }

    #[test]
    fn identity_projection_is_repeatable_bright_and_transparent_for_background() {
        let first = StableEntityId::new(7).unwrap();
        let second = StableEntityId::new(8).unwrap();
        let first_color = diagnostic_identity_color(first);
        assert_eq!(first_color, [225, 73, 101, 255]);
        assert_eq!(first_color, diagnostic_identity_color(first));
        assert_ne!(first_color, diagnostic_identity_color(second));
        assert!(first_color[..3].iter().all(|channel| *channel >= 64));

        let encoded = encode_diagnostic_png(
            dimensions(3, 1),
            DiagnosticPngSource::EntityId(&[None, Some(first), Some(second)]),
            &RuntimeLimits::default(),
            DiagnosticPngLimits::default(),
        )
        .unwrap();
        let (_, pixels, _) = decode(encoded);
        assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
        assert_eq!(&pixels[4..8], &first_color);
        assert_eq!(&pixels[8..12], &diagnostic_identity_color(second));
    }

    #[test]
    fn validation_rejects_kind_shape_runtime_and_encoded_size_boundaries() {
        let visibility = crate::ObservationPayload::Visibility(Vec::new());
        assert_eq!(
            DiagnosticPngSource::try_from(&visibility).unwrap_err(),
            DiagnosticPngError::UnsupportedKind {
                kind: cogniform_protocol::ObservationKind::Visibility
            }
        );
        assert_eq!(
            encode_diagnostic_png(
                dimensions(2, 1),
                DiagnosticPngSource::Depth(&[0.0]),
                &RuntimeLimits::default(),
                DiagnosticPngLimits::default(),
            )
            .unwrap_err(),
            DiagnosticPngError::ItemCountMismatch {
                expected: 2,
                actual: 1
            }
        );

        let runtime = RuntimeLimits {
            max_observation_pixels: NonZeroU64::new(1).unwrap(),
            ..RuntimeLimits::default()
        };
        assert_eq!(
            encode_diagnostic_png(
                dimensions(2, 1),
                DiagnosticPngSource::Depth(&[0.0, 1.0]),
                &runtime,
                DiagnosticPngLimits::default(),
            )
            .unwrap_err(),
            DiagnosticPngError::DimensionLimitExceeded
        );

        assert_eq!(
            encode_diagnostic_png(
                dimensions(1, 1),
                DiagnosticPngSource::Color(&[[1, 2, 3, 4]]),
                &RuntimeLimits::default(),
                DiagnosticPngLimits::new(NonZeroU64::new(1).unwrap()),
            )
            .unwrap_err(),
            DiagnosticPngError::PngLimitExceeded { limit: 1 }
        );
    }

    #[test]
    fn widest_named_profile_fits_the_default_png_bound() {
        let mut pixels = Vec::with_capacity(480 * 270);
        for index in 0_u32..(480 * 270) {
            let bytes = index.wrapping_mul(2_654_435_761).to_le_bytes();
            pixels.push([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        let encoded = encode_diagnostic_png(
            dimensions(480, 270),
            DiagnosticPngSource::Color(&pixels),
            &RuntimeLimits::default(),
            DiagnosticPngLimits::default(),
        )
        .unwrap();
        assert!(encoded.len() <= 1_048_576);
        let (info, decoded, _) = decode(encoded);
        assert_eq!((info.width, info.height), (480, 270));
        assert_eq!(decoded.len(), 480 * 270 * 4);
    }
}
