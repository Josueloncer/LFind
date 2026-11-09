//! Base64 estándar (RFC 4648) con relleno. Propio para no añadir una dependencia por 25 líneas.

const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Decodifica base64 estándar con relleno. `None` si no es válido.
pub fn decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if b.len() % 4 != 0 {
        return None;
    }
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(b.len() / 4 * 3);
    for (i, ch) in b.chunks_exact(4).enumerate() {
        let last = i + 1 == b.len() / 4;
        let pad = if last { ch.iter().rev().take_while(|c| **c == b'=').count() } else { 0 };
        if pad > 2 || ch[..4 - pad].contains(&b'=') {
            return None;
        }
        let mut n = 0u32;
        for c in &ch[..4 - pad] {
            n = (n << 6) | val(*c)?;
        }
        n <<= 6 * pad as u32;
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn rfc4648_vectors() {
        for (i, o) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(encode(i.as_bytes()), o);
        }
    }

    #[test]
    fn decode_reverses_encode_and_rejects_garbage() {
        for v in [&b""[..], b"f", b"fo", b"foo", b"foob", &[0, 255, 16, 200, 7]] {
            assert_eq!(decode(&encode(v)).unwrap(), v);
        }
        for bad in ["a", "abc", "ab=c", "====", "a$cd"] {
            assert!(decode(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn binary_data() {
        assert_eq!(encode(&[0xFF, 0xD8, 0xFF, 0x00, 0x10]), "/9j/ABA=");
        assert_eq!(encode(&(0..=255u8).collect::<Vec<_>>()).len(), 344);
    }
}
