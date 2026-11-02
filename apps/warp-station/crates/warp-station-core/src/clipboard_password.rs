//! Clipboard Password storage codec: the settings file keeps the password as
//! Base64 over UTF-8 — the exact legacy shape — and the hotkey and
//! drawer decode it right before the clipboard write.

/// Base64 (standard alphabet, padded) over UTF-8.
pub fn encode(plain: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = plain.as_bytes();
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Inverse of [`encode`]; `None` on any non-base64 byte or invalid UTF-8.
pub fn decode(stored: &str) -> Option<String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let value_of =
        |c: u8| -> Option<u32> { TABLE.iter().position(|&t| t == c).map(|ix| ix as u32) };
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for &c in stored.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        buffer = (buffer << 6) | value_of(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for plain in ["s3cret", "pä123 🎲", ""] {
            assert_eq!(decode(&encode(plain)).as_deref(), Some(plain));
        }
        assert_eq!(encode("hello warp-station"), "aGVsbG8gd2FycC1zdGF0aW9u");
        assert_eq!(decode("not@base64!"), None);
    }
}
