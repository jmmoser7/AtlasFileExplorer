use std::io::{self, Read};

use atlas_connector::*;
use serde_json::json;

#[test]
fn frame_header_body_and_json_failures_are_distinct() {
    assert!(read_frame::<serde_json::Value>(&mut &[][..])
        .unwrap()
        .is_none());
    for partial in 1..4 {
        assert!(
            matches!(read_frame::<serde_json::Value>(&mut &[1, 0, 0][..partial]), Err(FrameError::Truncated { expected: 4, received }) if received == partial)
        );
    }
    assert!(matches!(
        read_frame::<serde_json::Value>(&mut &[0, 0, 0, 0][..]),
        Err(FrameError::InvalidLength(0))
    ));
    assert!(matches!(
        read_frame::<serde_json::Value>(&mut &[3, 0, 0, 0, b'{'][..]),
        Err(FrameError::Truncated {
            expected: 3,
            received: 1
        })
    ));
    assert!(matches!(
        read_frame::<serde_json::Value>(&mut &[1, 0, 0, 0, 255][..]),
        Err(FrameError::Json(_))
    ));
}

struct HeaderOnly {
    bytes: [u8; 4],
    read: bool,
}
impl Read for HeaderOnly {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        assert!(!self.read, "oversized frame must not read its body");
        assert_eq!(buffer.len(), 4);
        buffer.copy_from_slice(&self.bytes);
        self.read = true;
        Ok(4)
    }
}

#[test]
fn oversize_is_rejected_before_body_read_and_writer_is_bounded() {
    let mut reader = HeaderOnly {
        bytes: (MAX_FRAME_BYTES as u32 + 1).to_le_bytes(),
        read: false,
    };
    assert!(matches!(
        read_frame::<serde_json::Value>(&mut reader),
        Err(FrameError::TooLarge { .. })
    ));
    let mut output = Vec::new();
    assert!(matches!(
        write_frame(&mut output, &"x".repeat(MAX_FRAME_BYTES)),
        Err(FrameError::TooLarge { .. })
    ));
    assert!(output.is_empty());
    write_raw_frame(&mut output, b"binary chunk").unwrap();
    assert!(matches!(
        read_raw_frame(&mut output.as_slice(), 2),
        Err(FrameError::TooLarge { limit: 2, .. })
    ));
}

#[test]
fn bounded_dialect_checks_types_required_enum_and_bounds_without_coercion() {
    let schema = json!({"type":"object", "additionalProperties":false,
        "properties":{"count":{"type":"integer", "minimum":1, "maximum":5},
        "mode":{"type":"string", "enum":["front","top"], "maxLength":5}}, "required":["count"]});
    validate_parameter_schema(&schema).unwrap();
    validate_parameters(&schema, &json!({"count":2,"mode":"front"})).unwrap();
    validate_parameters(&schema, &json!({"count":2.0})).unwrap();
    for params in [
        json!({}),
        json!({"count":"2"}),
        json!({"count":1.5}),
        json!({"count":6}),
        json!({"count":2,"mode":"back"}),
        json!({"count":2,"extra":true}),
    ] {
        assert_eq!(
            validate_parameters(&schema, &params).unwrap_err().code,
            ErrorCode::InvalidRequest
        );
    }
    let nested = json!({"type":"object", "properties":{"nested":{"type":"object"}}, "additionalProperties":false});
    assert_eq!(
        validate_parameter_schema(&nested).unwrap_err().code,
        ErrorCode::InvalidDescriptor
    );
    let reference = json!({"type":"object", "properties":{}, "additionalProperties":false, "$ref":"https://example.invalid/schema"});
    assert_eq!(
        validate_parameter_schema(&reference).unwrap_err().code,
        ErrorCode::InvalidDescriptor
    );
    let exact_large_integer = json!({"type":"object", "additionalProperties":false,
        "properties":{"count":{"type":"integer", "maximum":18446744073709551614u64}}});
    assert!(validate_parameters(
        &exact_large_integer,
        &json!({"count":18446744073709551615u64})
    )
    .is_err());
    let numeric_enum = json!({"type":"object", "additionalProperties":false,
        "properties":{"count":{"type":"integer", "enum":[2]}}});
    validate_parameters(&numeric_enum, &json!({"count":2.0})).unwrap();
}
