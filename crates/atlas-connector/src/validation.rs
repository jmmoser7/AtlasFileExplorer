use crate::*;

pub(crate) fn error(code: ErrorCode, message: &str) -> ProtocolError {
    ProtocolError::new(code, message)
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

pub(crate) fn validate_identity(id: &ConnectorIdentity) -> Result<(), ProtocolError> {
    if !valid_id(&id.pack_id)
        || !valid_id(&id.application_id)
        || id.application_version.is_empty()
        || id.application_version.len() > 128
        || id.application_version.chars().any(char::is_control)
        || id.connector_version.is_empty()
        || id.connector_version.len() > 128
        || id.connector_version.chars().any(char::is_control)
    {
        return Err(error(
            ErrorCode::InvalidDescriptor,
            "invalid connector identity",
        ));
    }
    Ok(())
}

pub(crate) fn validate_source(source: &SourceRef) -> Result<(), ProtocolError> {
    let locator = &source.locator;
    if !valid_id(&source.source_id) || source.revision.is_empty() || source.revision.len() > 256 {
        return Err(error(
            ErrorCode::InvalidRequest,
            "invalid source identity or revision",
        ));
    }
    if locator
        .root_id
        .as_deref()
        .is_some_and(|root| !valid_id(root))
        || locator.root_id.is_some() && locator.relative.is_none()
        || locator.preferred().is_none()
        || locator
            .relative
            .iter()
            .chain(locator.absolute.iter())
            .any(|s| s.is_empty() || s.len() > 32_768 || s.contains('\0'))
    {
        return Err(error(
            ErrorCode::InvalidRequest,
            "source requires a bounded relative-first locator",
        ));
    }
    if let Some(relative) = &locator.relative {
        if relative.starts_with(['/', '\\'])
            || relative.contains(':')
            || relative.split(['/', '\\']).any(|p| p == "..")
        {
            return Err(error(
                ErrorCode::InvalidRequest,
                "relative source locator escapes its root",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_command(command: &CommandDescriptor) -> Result<(), ProtocolError> {
    if !valid_id(&command.id)
        || !command.id.contains('.')
        || command.title.is_empty()
        || command.title.len() > 512
    {
        return Err(error(
            ErrorCode::InvalidDescriptor,
            "commands require bounded namespaced identity and title",
        ));
    }
    if matches!(
        command.effect,
        CommandEffect::DraftMutation | CommandEffect::SourceWriteBack
    ) && command.undo != UndoOwner::NativeApplication
        || command.effect == CommandEffect::SlateProposal && command.undo != UndoOwner::SlateJournal
    {
        return Err(error(
            ErrorCode::InvalidDescriptor,
            "command declares an inconsistent undo owner",
        ));
    }
    validate_parameter_schema(&command.parameters)
}

pub(crate) fn validate_schema(schema: &ViewSchema) -> Result<(), ProtocolError> {
    if !valid_id(&schema.namespace) || !schema.namespace.contains('.') || schema.version == 0 {
        return Err(error(
            ErrorCode::UnsupportedSchema,
            "view schema requires a namespaced identity and positive version",
        ));
    }
    Ok(())
}

pub(crate) fn validate_view(
    view: &ViewSnapshot,
    scope: &SourceScope,
    schemas: &[ViewSchema],
) -> Result<(), ProtocolError> {
    let schema = ViewSchema {
        namespace: view.schema.clone(),
        version: view.schema_version,
    };
    validate_schema(&schema)?;
    if !schemas.contains(&schema) {
        return Err(error(
            ErrorCode::UnsupportedSchema,
            "view schema was not advertised",
        ));
    }
    if &view.source != scope {
        return Err(error(
            ErrorCode::SourceMismatch,
            "view belongs to another source or revision",
        ));
    }
    if serde_json::to_vec(&view.state).map_or(true, |bytes| bytes.len() > MAX_VIEW_BYTES) {
        return Err(error(
            ErrorCode::InvalidRequest,
            "view state exceeds size limit",
        ));
    }
    Ok(())
}

pub(crate) fn validate_artifact(artifact: &ArtifactTransfer) -> Result<(), ProtocolError> {
    if artifact.byte_length == 0 || artifact.byte_length > MAX_ARTIFACT_BYTES {
        return Err(error(
            ErrorCode::TransferTooLarge,
            "artifact length outside 1..64MiB",
        ));
    }
    let mut mime_parts = artifact.media_type.split('/');
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&b))
    };
    let valid_mime = mime_parts.next().is_some_and(token)
        && mime_parts.next().is_some_and(token)
        && mime_parts.next().is_none();
    if !valid_id(&artifact.transfer_id)
        || artifact.media_type.len() > 128
        || !valid_mime
        || artifact.sha256.len() != 64
        || !artifact
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(error(
            ErrorCode::InvalidResult,
            "invalid artifact transfer descriptor",
        ));
    }
    Ok(())
}

pub(crate) fn validate_capture(
    capture: &CaptureResult,
    scope: &SourceScope,
    schemas: &[ViewSchema],
) -> Result<(), ProtocolError> {
    let meta = &capture.metadata;
    if &meta.source != scope || meta.view.source != meta.source {
        return Err(error(
            ErrorCode::SourceMismatch,
            "capture pixels/source/view must name one revision",
        ));
    }
    validate_view(&meta.view, scope, schemas)?;
    if meta.width_px == 0
        || meta.height_px == 0
        || meta.width_px > MAX_CAPTURE_DIMENSION
        || meta.height_px > MAX_CAPTURE_DIMENSION
        || !valid_utc_timestamp(&meta.captured_at)
        || !matches!(
            capture.artifact.media_type.as_str(),
            "image/png" | "image/jpeg" | "image/webp"
        )
    {
        return Err(error(
            ErrorCode::InvalidResult,
            "invalid still capture metadata",
        ));
    }
    validate_artifact(&capture.artifact)
}

// The wire profile uses RFC3339 UTC seconds with optional 1..9 fractional
// digits. Leap-second notation is outside this capture timestamp profile.
pub(crate) fn valid_utc_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !(20..=30).contains(&bytes.len())
        || bytes.last() != Some(&b'Z')
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return false;
    }
    let number = |start: usize, end: usize| -> Option<u32> {
        let slice = bytes.get(start..end)?;
        if !slice.iter().all(u8::is_ascii_digit) {
            return None;
        }
        slice.iter().try_fold(0u32, |n, b| {
            n.checked_mul(10)?.checked_add((b - b'0') as u32)
        })
    };
    let Some((year, month, day, hour, minute, second)) = number(0, 4)
        .zip(number(5, 7))
        .zip(number(8, 10))
        .zip(number(11, 13))
        .zip(number(14, 16))
        .zip(number(17, 19))
        .map(|(((((y, mo), d), h), mi), s)| (y, mo, d, h, mi, s))
    else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    year != 0
        && day != 0
        && day <= days
        && hour < 24
        && minute < 60
        && second < 60
        && (bytes.len() == 20
            || bytes.len() >= 22
                && bytes.get(19) == Some(&b'.')
                && bytes[20..bytes.len() - 1].iter().all(u8::is_ascii_digit))
}

pub(crate) fn validate_surface(
    action: &SurfaceAction,
    modes: &[SurfaceMode],
) -> Result<(), ProtocolError> {
    match action {
        SurfaceAction::Attach { surface } => {
            validate_surface_descriptor(surface, modes)?;
        }
        SurfaceAction::Place { geometry }
            if geometry.width_px == 0
                || geometry.height_px == 0
                || geometry.width_px > 65_536
                || geometry.height_px > 65_536
                || !(48..=960).contains(&geometry.dpi) =>
        {
            return Err(error(
                ErrorCode::InvalidRequest,
                "invalid physical surface geometry or DPI",
            ));
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn validate_surface_descriptor(
    surface: &SurfaceDescriptor,
    modes: &[SurfaceMode],
) -> Result<(), ProtocolError> {
    if !valid_id(&surface.surface_id) || !modes.contains(&surface.mode) || !surface.can_detach {
        return Err(error(
            ErrorCode::InvalidRequest,
            "surface mode unoffered or detach recovery unavailable",
        ));
    }
    let native_mode = matches!(
        surface.mode,
        SurfaceMode::NativeFullUi | SurfaceMode::NativeViewport
    );
    if native_mode && surface.native_handle.is_none()
        || surface.native_handle.as_deref().is_some_and(|handle| {
            handle.len() > 20
                || !handle.bytes().all(|b| b.is_ascii_digit())
                || handle.parse::<u64>().ok().is_none_or(|n| n == 0)
        })
    {
        return Err(error(
            ErrorCode::InvalidRequest,
            "native surface requires a bounded nonzero decimal runtime handle",
        ));
    }
    Ok(())
}
