use std::io::{self, Write};

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(B64[((n >> 18) & 0x3F) as usize] as char);
        out.push(B64[((n >> 12) & 0x3F) as usize] as char);
        out.push(B64[((n >> 6) & 0x3F) as usize] as char);
        out.push(B64[(n & 0x3F) as usize] as char);
        i += 3;
    }
    match bytes.len() - i {
        1 => {
            let n = (bytes[i] as u32) << 16;
            out.push(B64[((n >> 18) & 0x3F) as usize] as char);
            out.push(B64[((n >> 12) & 0x3F) as usize] as char);
            out.push_str("==");
        }
        2 => {
            let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
            out.push(B64[((n >> 18) & 0x3F) as usize] as char);
            out.push(B64[((n >> 12) & 0x3F) as usize] as char);
            out.push(B64[((n >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

pub fn write_osc52(writer: &mut dyn Write, text: &str) -> io::Result<()> {
    let encoded = b64_encode(text.as_bytes());
    write!(writer, "\x1b]52;c;{encoded}\x07")?;
    writer.flush()
}

pub fn copy_to_clipboard(text: &str) -> io::Result<()> {
    write_osc52(&mut io::stdout().lock(), text)
}
