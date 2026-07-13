use beryl_model::{
    MonitorHint, MonitorId, VirtualDesktopId, WindowBounds, WindowDisplayState, WindowPlacement,
};

use crate::encoding::{CodecError, Decoder, Encoder};

use super::{invalid, invariant};

pub(super) fn encode_placement(encoder: &mut Encoder, value: &WindowPlacement) {
    encode_bounds(encoder, value.bounds());
    encoder.u8(match value.display_state() {
        WindowDisplayState::Normal => 0,
        WindowDisplayState::Maximized => 1,
    });
    match value.monitor() {
        Some(monitor) => {
            encoder.u8(1);
            encoder.u16(monitor.id().as_str().len() as u16);
            encoder.padded(monitor.id().as_str().as_bytes(), MonitorId::MAX_BYTES);
            encode_bounds(encoder, monitor.work_area());
        }
        None => {
            encoder.u8(0);
            encoder.u16(0);
            encoder.padded(&[], MonitorId::MAX_BYTES);
            encoder.padded(&[], 16);
        }
    }
    match value.virtual_desktop() {
        Some(identity) => {
            encoder.u8(1);
            encoder.fixed(identity.as_bytes());
        }
        None => {
            encoder.u8(0);
            encoder.fixed(&[0; 16]);
        }
    }
}

pub(super) fn decode_placement(decoder: &mut Decoder<'_>) -> Result<WindowPlacement, CodecError> {
    let bounds = decode_bounds(decoder, "window bounds")?;
    let display_state = match decoder.u8()? {
        0 => WindowDisplayState::Normal,
        1 => WindowDisplayState::Maximized,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "window display state",
                tag,
            });
        }
    };
    let monitor_tag = decoder.u8()?;
    let monitor_length = usize::from(decoder.u16()?);
    let monitor_bytes = decoder.bytes(MonitorId::MAX_BYTES)?;
    let monitor_bounds = decode_raw_bounds(decoder)?;
    let monitor = match monitor_tag {
        0 if monitor_length == 0
            && monitor_bytes.iter().all(|byte| *byte == 0)
            && monitor_bounds == (0, 0, 0, 0) =>
        {
            None
        }
        0 => return Err(invariant("absent monitor hint has nonzero padding")),
        1 if (1..=MonitorId::MAX_BYTES).contains(&monitor_length) => {
            if monitor_bytes[monitor_length..]
                .iter()
                .any(|byte| *byte != 0)
            {
                return Err(invariant("monitor identity has nonzero padding"));
            }
            let identity = std::str::from_utf8(&monitor_bytes[..monitor_length]).map_err(|_| {
                CodecError::InvalidUtf8 {
                    kind: "monitor identity",
                }
            })?;
            let identity =
                MonitorId::new(identity).map_err(|source| invalid("monitor identity", source))?;
            Some(MonitorHint::new(
                identity,
                bounds_from_raw(monitor_bounds, "monitor work area")?,
            ))
        }
        1 => {
            return Err(invariant(
                "monitor identity length is outside its fixed capacity",
            ));
        }
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "optional monitor hint",
                tag,
            });
        }
    };
    let desktop_tag = decoder.u8()?;
    let desktop = decoder.fixed()?;
    let virtual_desktop = match desktop_tag {
        0 if desktop == [0; 16] => None,
        0 => return Err(invariant("absent virtual desktop has nonzero padding")),
        1 => Some(VirtualDesktopId::from_bytes(desktop)),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "optional virtual desktop",
                tag,
            });
        }
    };
    Ok(WindowPlacement::new(
        bounds,
        display_state,
        monitor,
        virtual_desktop,
    ))
}

fn encode_bounds(encoder: &mut Encoder, value: WindowBounds) {
    encoder.i32(value.x());
    encoder.i32(value.y());
    encoder.u32(value.width());
    encoder.u32(value.height());
}

fn decode_bounds(
    decoder: &mut Decoder<'_>,
    kind: &'static str,
) -> Result<WindowBounds, CodecError> {
    bounds_from_raw(decode_raw_bounds(decoder)?, kind)
}

fn decode_raw_bounds(decoder: &mut Decoder<'_>) -> Result<(i32, i32, u32, u32), CodecError> {
    Ok((
        decoder.i32()?,
        decoder.i32()?,
        decoder.u32()?,
        decoder.u32()?,
    ))
}

fn bounds_from_raw(
    (x, y, width, height): (i32, i32, u32, u32),
    kind: &'static str,
) -> Result<WindowBounds, CodecError> {
    WindowBounds::new(x, y, width, height).map_err(|source| invalid(kind, source))
}
