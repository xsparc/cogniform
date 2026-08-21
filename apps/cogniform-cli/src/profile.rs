//! Closed launch-time profiles for inherited-standard-stream services.

use std::{
    ffi::{OsStr, OsString},
    io,
};

pub(crate) const PROFILE_NAMES: &str = "default-local-64x64|local-256x256|local-480x270";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalProfile {
    DefaultLocal64x64,
    Local256x256,
    Local480x270,
}

impl LocalProfile {
    pub(crate) const DEFAULT: Self = Self::DefaultLocal64x64;
    pub(crate) const ALL: [Self; 3] = [
        Self::DefaultLocal64x64,
        Self::Local256x256,
        Self::Local480x270,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::DefaultLocal64x64 => "default-local-64x64",
            Self::Local256x256 => "local-256x256",
            Self::Local480x270 => "local-480x270",
        }
    }

    pub(crate) const fn dimensions(self) -> (u32, u32) {
        match self {
            Self::DefaultLocal64x64 => (64, 64),
            Self::Local256x256 => (256, 256),
            Self::Local480x270 => (480, 270),
        }
    }

    pub(crate) fn parse(
        arguments: &mut impl Iterator<Item = OsString>,
        command: &str,
    ) -> Result<Self, io::Error> {
        let Some(flag) = arguments.next() else {
            return Ok(Self::DEFAULT);
        };
        let profile = if flag == OsStr::new("--profile") {
            arguments
                .next()
                .as_deref()
                .and_then(Self::from_name)
                .ok_or_else(|| invalid_arguments(command))?
        } else {
            return Err(invalid_arguments(command));
        };
        if arguments.next().is_some() {
            return Err(invalid_arguments(command));
        }
        Ok(profile)
    }

    fn from_name(name: &OsStr) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|profile| name == OsStr::new(profile.name()))
    }
}

fn invalid_arguments(command: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{command} accepts only optional --profile <{PROFILE_NAMES}>"),
    )
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU32;
    use std::ffi::OsString;

    use cogniform_local_transport::LocalFrameConfig;
    use cogniform_mcp::McpTransportLimits;
    use cogniform_observation::{
        OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES, ObservationPayload, encode_payload,
    };
    use cogniform_protocol::{
        FrameId, ImageDimensions, ObservationId, ObservationKind, ObservationMetadata,
        ObservationQuality, ObservationStaleness, RuntimeLimits, SceneRevision, SchemaVersion,
        StableEntityId,
    };

    use super::{LocalProfile, PROFILE_NAMES};

    const MAX_ENTITY_ID_ITEM_BYTES: u64 = 17;
    const EXPECTED_WIDE_ENTITY_ENVELOPE_BYTES: u64 = 2_203_260;
    const EXPECTED_WIDE_ENTITY_BASE64_BYTES: u64 = 2_937_680;

    #[test]
    fn exact_profile_names_map_to_immutable_dimensions() {
        assert_eq!(
            LocalProfile::ALL.map(|profile| (profile.name(), profile.dimensions())),
            [
                ("default-local-64x64", (64, 64)),
                ("local-256x256", (256, 256)),
                ("local-480x270", (480, 270)),
            ]
        );
        assert_eq!(LocalProfile::DEFAULT, LocalProfile::DefaultLocal64x64);
    }

    #[test]
    fn parser_accepts_only_omission_or_one_exact_named_profile() {
        assert_eq!(parse(&[]).unwrap(), LocalProfile::DEFAULT);
        for profile in LocalProfile::ALL {
            assert_eq!(parse(&["--profile", profile.name()]).unwrap(), profile);
        }
    }

    #[test]
    fn parser_rejects_every_other_argument_shape_without_echoing_input() {
        let non_unicode = non_unicode_argument();
        let cases = [
            vec![OsString::from("unexpected")],
            vec![OsString::from("--help")],
            vec![OsString::from("--profile")],
            vec![OsString::from("--profile"), OsString::from("unknown")],
            vec![OsString::from("local-256x256")],
            vec![
                OsString::from("--profile"),
                OsString::from("local-256x256"),
                OsString::from("extra"),
            ],
            vec![
                OsString::from("--profile"),
                OsString::from("local-256x256"),
                OsString::from("--profile"),
                OsString::from("local-480x270"),
            ],
            vec![OsString::from("--profile"), non_unicode],
        ];
        let expected = format!("serve-stdio accepts only optional --profile <{PROFILE_NAMES}>");
        for case in cases {
            let mut arguments = case.into_iter();
            let error = LocalProfile::parse(&mut arguments, "serve-stdio").unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn every_profile_fits_runtime_payload_and_transport_output_bounds() {
        let runtime = RuntimeLimits::default();
        let frame = LocalFrameConfig::default();
        let mcp = McpTransportLimits::default();
        for profile in LocalProfile::ALL {
            let (width, height) = profile.dimensions();
            let pixels = u64::from(width) * u64::from(height);
            assert!(width <= runtime.max_observation_width.get());
            assert!(height <= runtime.max_observation_height.get());
            assert!(pixels <= runtime.max_observation_pixels.get());

            let envelope_bytes = u64::try_from(OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES)
                .unwrap()
                .checked_add(pixels.checked_mul(MAX_ENTITY_ID_ITEM_BYTES).unwrap())
                .unwrap();
            assert!(envelope_bytes <= frame.payload_limits.max_envelope_bytes.get());
            assert!(envelope_bytes <= frame.frame_limits.max_bulk_bytes.get());
            let base64_bytes = envelope_bytes.div_ceil(3).checked_mul(4).unwrap();
            assert!(base64_bytes < mcp.max_output_bytes.get());
        }

        let (width, height) = LocalProfile::Local480x270.dimensions();
        let pixels = usize::try_from(u64::from(width) * u64::from(height)).unwrap();
        let metadata = image_metadata(width, height);
        let encoded = encode_payload(
            &metadata,
            &ObservationPayload::EntityId(vec![None; pixels]),
            &runtime,
            frame.payload_limits,
        )
        .unwrap();
        assert_eq!(
            u64::try_from(encoded.len()).unwrap(),
            EXPECTED_WIDE_ENTITY_ENVELOPE_BYTES
        );
        assert_eq!(
            u64::try_from(encoded.len()).unwrap().div_ceil(3) * 4,
            EXPECTED_WIDE_ENTITY_BASE64_BYTES
        );
    }

    fn parse(arguments: &[&str]) -> Result<LocalProfile, std::io::Error> {
        let mut arguments = arguments.iter().map(OsString::from);
        LocalProfile::parse(&mut arguments, "serve-stdio")
    }

    fn image_metadata(width: u32, height: u32) -> ObservationMetadata {
        ObservationMetadata {
            schema_version: SchemaVersion::V1,
            observation_id: ObservationId::new(1).unwrap(),
            scene_revision: SceneRevision::new(1),
            frame_id: FrameId::new(1).unwrap(),
            camera_id: StableEntityId::new(1).unwrap(),
            kind: ObservationKind::EntityId,
            dimensions: Some(ImageDimensions {
                width: NonZeroU32::new(width).unwrap(),
                height: NonZeroU32::new(height).unwrap(),
            }),
            quality: ObservationQuality::Low,
            observed_at_unix_micros: 0,
            production_latency_micros: 0,
            staleness: ObservationStaleness {
                latest_known_revision: SceneRevision::new(1),
                revisions_behind: 0,
            },
        }
    }

    #[cfg(unix)]
    fn non_unicode_argument() -> OsString {
        use std::os::unix::ffi::OsStringExt as _;

        OsString::from_vec(vec![0xff])
    }

    #[cfg(windows)]
    fn non_unicode_argument() -> OsString {
        use std::os::windows::ffi::OsStringExt as _;

        OsString::from_wide(&[0xd800])
    }
}
