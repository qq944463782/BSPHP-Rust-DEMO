//! 与 AppEn 请求体字段值的百分号编码一致（字母数字与 -._~ 不编码）。

pub fn encode_parameter(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for ch in s.chars() {
        let o = ch as u32;
        if (65..=90).contains(&o)
            || (97..=122).contains(&o)
            || (48..=57).contains(&o)
            || matches!(ch, '-' | '.' | '_' | '~')
        {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            let utf8 = ch.encode_utf8(&mut buf);
            use std::fmt::Write;
            for b in utf8.bytes() {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// 与 `urllib.parse.quote(s, safe="-_.~")` 一致，用于整段 `parameter=` 负载。
pub fn quote_parameter_payload(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_ascii_and_utf8() {
        assert_eq!(encode_parameter("a-z_0.~"), "a-z_0.~");
        assert_eq!(encode_parameter(" "), "%20");
        assert_eq!(encode_parameter("中"), "%E4%B8%AD");
    }

    #[test]
    fn quote_payload_percent_encodes_plus_and_pipe() {
        let s = "abc+|";
        assert_eq!(quote_parameter_payload(s), "abc%2B%7C");
    }
}

