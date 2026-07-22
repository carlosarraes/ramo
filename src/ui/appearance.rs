use super::themes::TerminalAppearance;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

pub fn parse_osc11_background(bytes: &[u8]) -> Option<RgbColor> {
    let marker = b"\x1b]11;";
    let start = bytes
        .windows(marker.len())
        .position(|window| window == marker)?
        + marker.len();
    let value = &bytes[start..];
    let end = value
        .iter()
        .position(|byte| *byte == 0x07)
        .or_else(|| value.windows(2).position(|window| window == b"\x1b\\"))?;
    let value = std::str::from_utf8(&value[..end]).ok()?;
    if let Some(channels) = value.strip_prefix("rgb:") {
        let mut channels = channels.split('/');
        let red = parse_hex_channel(channels.next()?)?;
        let green = parse_hex_channel(channels.next()?)?;
        let blue = parse_hex_channel(channels.next()?)?;
        if channels.next().is_some() {
            return None;
        }
        return Some(RgbColor { red, green, blue });
    }
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(RgbColor {
        red: u8::from_str_radix(&hex[..2], 16).ok()?,
        green: u8::from_str_radix(&hex[2..4], 16).ok()?,
        blue: u8::from_str_radix(&hex[4..], 16).ok()?,
    })
}

pub fn appearance_for_background(color: RgbColor) -> TerminalAppearance {
    let linear = [color.red, color.green, color.blue].map(|component| {
        let normalized = f64::from(component) / 255.0;
        if normalized <= 0.039_28 {
            normalized / 12.92
        } else {
            ((normalized + 0.055) / 1.055).powf(2.4)
        }
    });
    let luminance = 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
    if luminance > 0.5 {
        TerminalAppearance::Light
    } else {
        TerminalAppearance::Dark
    }
}

pub fn appearance_from_colorfgbg(value: &str) -> Option<TerminalAppearance> {
    let background = value.rsplit(';').next()?.trim().parse::<u8>().ok()?;
    Some(if matches!(background, 7 | 15) {
        TerminalAppearance::Light
    } else {
        TerminalAppearance::Dark
    })
}

pub fn detect_terminal_appearance() -> Option<TerminalAppearance> {
    std::env::var("COLORFGBG")
        .ok()
        .as_deref()
        .and_then(appearance_from_colorfgbg)
}

fn parse_hex_channel(channel: &str) -> Option<u8> {
    if !(2..=4).contains(&channel.len()) || !channel.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(channel, 16).ok()?;
    let maximum = 16_u32.pow(channel.len() as u32) - 1;
    Some(((value * 255 + maximum / 2) / maximum) as u8)
}
