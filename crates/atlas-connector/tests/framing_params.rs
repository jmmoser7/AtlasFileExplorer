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

fn json_string_schema() -> serde_json::Value {
    json!({"type":"object","additionalProperties":false,"required":["plan"],
        "properties":{"plan":{"type":"string","contentMediaType":"application/json","maxLength":24576}}})
}

#[test]
fn annotated_json_strings_have_a_local_byte_limit_and_require_json() {
    let schema = json_string_schema();
    validate_parameter_schema(&schema).unwrap();
    for length in [4097, 24576] {
        let encoded = format!("\"{}\"", "x".repeat(length - 2));
        assert_eq!(encoded.len(), length);
        validate_parameters(&schema, &json!({"plan":encoded})).unwrap();
    }
    for invalid in [
        "Create a model".to_owned(),
        "{\"kind\":\"mesh\"".to_owned(),
        format!("\"{}\"", "x".repeat(24575)),
        // Fewer than 24576 characters still exceeds the UTF-8 byte cap.
        serde_json::to_string(&"é".repeat(12288)).unwrap(),
    ] {
        assert_eq!(
            validate_parameters(&schema, &json!({"plan":invalid}))
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
    }
    let mut ordinary = schema.clone();
    ordinary["properties"]["plan"]
        .as_object_mut()
        .unwrap()
        .remove("contentMediaType");
    assert_eq!(
        validate_parameter_schema(&ordinary).unwrap_err().code,
        ErrorCode::InvalidDescriptor
    );
    ordinary["properties"]["plan"]["maxLength"] = json!(4096);
    assert!(validate_parameters(&ordinary, &json!({"plan":"x".repeat(4097)})).is_err());
    validate_parameters(&ordinary, &json!({"plan":"x".repeat(4096)})).unwrap();
}

#[test]
fn content_annotation_is_closed_and_does_not_enable_nested_schemas() {
    for annotation in [
        json!("text/plain"),
        json!("Application/JSON"),
        json!(true),
        json!(null),
    ] {
        let mut schema = json_string_schema();
        schema["properties"]["plan"]["contentMediaType"] = annotation;
        assert!(validate_parameter_schema(&schema).is_err());
    }
    let mut nonstring = json_string_schema();
    nonstring["properties"]["plan"]["type"] = json!("object");
    assert!(validate_parameter_schema(&nonstring).is_err());
    let mut oversize = json_string_schema();
    oversize["properties"]["plan"]["maxLength"] = json!(24577);
    assert!(validate_parameter_schema(&oversize).is_err());
    let mut enum_schema = json_string_schema();
    enum_schema["properties"]["plan"]["enum"] = json!(["not JSON"]);
    assert!(validate_parameter_schema(&enum_schema).is_err());
}

#[test]
fn annotated_json_keeps_the_exact_serialized_parameter_envelope_bound() {
    let mut schema = json_string_schema();
    let shape = schema["properties"]["plan"].clone();
    schema["properties"]["second"] = shape.clone();
    schema["properties"]["third"] = shape;
    // Escaped quotes/backslashes are part of the outer parameter byte count.
    let encoded = serde_json::to_string(&"\\\"".repeat(5000)).unwrap();
    let one = json!({"plan":encoded});
    assert!(serde_json::to_vec(&one).unwrap().len() > encoded.len());
    validate_parameters(&schema, &one).unwrap();
    let encoded = format!("\"{}\"", "x".repeat(24000));
    let two = json!({"plan":encoded,"second":encoded});
    assert!(serde_json::to_vec(&two).unwrap().len() < MAX_PARAMETER_BYTES);
    validate_parameters(&schema, &two).unwrap();
    let three = json!({"plan":encoded,"second":encoded,"third":encoded});
    assert!(serde_json::to_vec(&three).unwrap().len() > MAX_PARAMETER_BYTES);
    assert_eq!(
        validate_parameters(&schema, &three).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(MAX_PARAMETER_STRING, 4096);
    assert_eq!(MAX_PARAMETER_BYTES, 65536);
}
