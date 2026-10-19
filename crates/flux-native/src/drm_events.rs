//! Checked Linux DRM event batches. The DRM read contract returns complete
//! events; this decoder deliberately rejects a truncated or malformed batch.
use serde::Serialize;
pub const MAX_EVENT_BYTES: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Flip {
    #[serde(serialize_with = "decimal")]
    pub cookie: u64,
    pub crtc: u32,
    pub sequence: u32,
    pub seconds: u32,
    pub microseconds: u32,
}
fn decimal<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Oversized,
    Truncated,
    InvalidLength,
    InvalidFlipLength,
    MissingCrtc,
    InvalidTimestamp,
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_ne_bytes(
        bytes[at..at + 4]
            .try_into()
            .expect("validated event bounds"),
    )
}
/// Validates the entire read before returning any completion. Unknown event
/// kinds are skipped; ordinary VBLANK events never become flip completions.
pub fn decode(bytes: &[u8]) -> Result<Vec<Flip>, DecodeError> {
    if bytes.len() > MAX_EVENT_BYTES {
        return Err(DecodeError::Oversized);
    }
    let mut result = vec![];
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes.len() - cursor < 8 {
            return Err(DecodeError::Truncated);
        }
        let kind = u32_at(bytes, cursor);
        let length = u32_at(bytes, cursor + 4) as usize;
        if length < 8 {
            return Err(DecodeError::InvalidLength);
        }
        let end = cursor
            .checked_add(length)
            .ok_or(DecodeError::InvalidLength)?;
        if end > bytes.len() {
            return Err(DecodeError::Truncated);
        }
        if kind == 2 {
            // DRM_EVENT_FLIP_COMPLETE / struct drm_event_vblank
            if length != 32 {
                return Err(DecodeError::InvalidFlipLength);
            }
            let crtc = u32_at(bytes, cursor + 28);
            let microseconds = u32_at(bytes, cursor + 20);
            if crtc == 0 {
                return Err(DecodeError::MissingCrtc);
            }
            if microseconds >= 1_000_000 {
                return Err(DecodeError::InvalidTimestamp);
            }
            result.push(Flip {
                cookie: u64::from_ne_bytes(
                    bytes[cursor + 8..cursor + 16]
                        .try_into()
                        .expect("validated flip bounds"),
                ),
                crtc,
                sequence: u32_at(bytes, cursor + 24),
                seconds: u32_at(bytes, cursor + 16),
                microseconds,
            });
        }
        cursor = end;
    }
    Ok(result)
}
#[cfg(test)]
pub(crate) fn encoded(cookie: u64, crtc: u32, sequence: u32) -> Vec<u8> {
    let mut bytes = vec![];
    bytes.extend(2u32.to_ne_bytes());
    bytes.extend(32u32.to_ne_bytes());
    bytes.extend(cookie.to_ne_bytes());
    bytes.extend(7u32.to_ne_bytes());
    bytes.extend(999_999u32.to_ne_bytes());
    bytes.extend(sequence.to_ne_bytes());
    bytes.extend(crtc.to_ne_bytes());
    bytes
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_full_cookie_unaligned_events_and_sequence_wrap() {
        let mut bytes = vec![];
        bytes.extend(99u32.to_ne_bytes());
        bytes.extend(9u32.to_ne_bytes());
        bytes.push(0);
        bytes.extend(encoded(u64::MAX, 5, u32::MAX));
        bytes.extend(encoded(1, 5, 0));
        let events = decode(&bytes).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].cookie, u64::MAX);
        assert_eq!(events[1].sequence, 0);
        assert!(
            serde_json::to_string(&events[0])
                .unwrap()
                .contains("\"18446744073709551615\"")
        );
    }
    #[test]
    fn malformed_tail_never_delivers_valid_prefix() {
        let mut bytes = encoded(9, 5, 1);
        bytes.extend([0; 7]);
        assert_eq!(decode(&bytes), Err(DecodeError::Truncated));
        for length in 0..8 {
            let mut bytes = encoded(9, 5, 1);
            bytes[4..8].copy_from_slice(&(length as u32).to_ne_bytes());
            assert!(decode(&bytes).is_err());
        }
        assert_eq!(decode(&encoded(9, 0, 1)), Err(DecodeError::MissingCrtc));
        let mut bytes = encoded(9, 5, 1);
        bytes[20..24].copy_from_slice(&1_000_000u32.to_ne_bytes());
        assert_eq!(decode(&bytes), Err(DecodeError::InvalidTimestamp));
    }
    #[test]
    fn bounded_arbitrary_inputs_and_all_truncations_do_not_panic() {
        let packet = encoded(9, 5, 1);
        for end in 1..packet.len() {
            assert!(decode(&packet[..end]).is_err());
        }
        assert_eq!(
            decode(&vec![0; MAX_EVENT_BYTES + 1]),
            Err(DecodeError::Oversized)
        );
        let mut random = 1u64;
        for length in 0..=MAX_EVENT_BYTES {
            let mut bytes = vec![0; length];
            for byte in &mut bytes {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                *byte = random as u8;
            }
            let _ = decode(&bytes);
        }
    }
    #[test]
    fn vblank_and_unknown_events_do_not_authorize_a_flip() {
        let mut bytes = encoded(9, 5, 1);
        bytes[..4].copy_from_slice(&1u32.to_ne_bytes());
        assert!(decode(&bytes).unwrap().is_empty());
        assert!(decode(&[]).unwrap().is_empty());
    }
}
