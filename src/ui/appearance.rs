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
    probe_terminal_background().or_else(|| {
        std::env::var("COLORFGBG")
            .ok()
            .as_deref()
            .and_then(appearance_from_colorfgbg)
    })
}

fn parse_hex_channel(channel: &str) -> Option<u8> {
    if !(2..=4).contains(&channel.len()) || !channel.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(channel, 16).ok()?;
    let maximum = 16_u32.pow(channel.len() as u32) - 1;
    Some(((value * 255 + maximum / 2) / maximum) as u8)
}

#[cfg(unix)]
fn probe_terminal_background() -> Option<TerminalAppearance> {
    use std::fs::OpenOptions;
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};

    const QUERY: &[u8] = b"\x1b]11;?\x1b\\";
    const TIMEOUT: Duration = Duration::from_millis(150);

    struct ModeGuard {
        fd: std::os::fd::RawFd,
        original: libc::termios,
    }
    impl Drop for ModeGuard {
        fn drop(&mut self) {
            unsafe {
                libc::tcsetattr(self.fd, libc::TCSANOW, &self.original);
            }
        }
    }

    let mut tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let fd = tty.as_raw_fd();
    let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
    if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
        return None;
    }
    let original = unsafe { original.assume_init() };
    let mut raw = original;
    unsafe {
        libc::cfmakeraw(&mut raw);
    }
    raw.c_cc[libc::VMIN] = 0;
    raw.c_cc[libc::VTIME] = 0;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
        return None;
    }
    let _guard = ModeGuard { fd, original };
    tty.write_all(QUERY).ok()?;
    tty.flush().ok()?;

    let deadline = Instant::now() + TIMEOUT;
    let mut response = Vec::with_capacity(64);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        let mut descriptor = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let timeout = remaining.as_millis().min(i32::MAX as u128) as i32;
        let ready = unsafe { libc::poll(&mut descriptor, 1, timeout) };
        if ready < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return None;
        }
        if ready == 0 || descriptor.revents & libc::POLLIN == 0 {
            return None;
        }
        let mut chunk = [0_u8; 64];
        let count = tty.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        response.extend_from_slice(&chunk[..count]);
        if let Some(color) = parse_osc11_background(&response) {
            return Some(appearance_for_background(color));
        }
        if response.len() >= 512 {
            return None;
        }
    }
}

#[cfg(windows)]
fn probe_terminal_background() -> Option<TerminalAppearance> {
    None
}
