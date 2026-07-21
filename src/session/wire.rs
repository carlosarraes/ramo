use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{SESSION_REGISTRATION_VERSION, SessionRegistration, SessionSnapshot};

pub const SESSION_WIRE_PREFACE: &[u8] = b"PDIFF-SESSION/1\n";
pub const MAX_SESSION_FRAME_BYTES: usize = 1024 * 1024;

pub(crate) fn connection_uses_session_wire(stream: &TcpStream) -> io::Result<bool> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut bytes = [0_u8; SESSION_WIRE_PREFACE.len()];
    loop {
        let count = stream.peek(&mut bytes)?;
        if count > 0 && bytes[..count] != SESSION_WIRE_PREFACE[..count] {
            return Ok(false);
        }
        if count == SESSION_WIRE_PREFACE.len() {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timed out reading pdiff session connection preface",
            ));
        }
        thread::sleep(Duration::from_millis(2));
    }
}

pub trait SessionWireFrame: Serialize + DeserializeOwned {
    fn validate_version(&self) -> io::Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientSessionFrame {
    Register {
        version: u32,
        registration: SessionRegistration,
        snapshot: SessionSnapshot,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_path: Option<String>,
    },
    Snapshot {
        version: u32,
        snapshot: SessionSnapshot,
    },
    Registration {
        version: u32,
        registration: SessionRegistration,
    },
    CommandResult {
        version: u32,
        request_id: String,
        result: Result<Value, String>,
        snapshot: SessionSnapshot,
    },
    Ping,
    Unregister,
}

impl SessionWireFrame for ClientSessionFrame {
    fn validate_version(&self) -> io::Result<()> {
        let version = match self {
            Self::Register { version, .. }
            | Self::Snapshot { version, .. }
            | Self::Registration { version, .. }
            | Self::CommandResult { version, .. } => Some(*version),
            Self::Ping | Self::Unregister => None,
        };
        if version.is_some_and(|version| version != SESSION_REGISTRATION_VERSION) {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "incompatible pdiff session registration frame version",
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ServerSessionFrame {
    Registered {
        version: u32,
        generation: u64,
    },
    Command {
        version: u32,
        request_id: String,
        input: Value,
    },
    Pong,
    Error {
        version: u32,
        message: String,
    },
}

impl SessionWireFrame for ServerSessionFrame {
    fn validate_version(&self) -> io::Result<()> {
        let version = match self {
            Self::Registered { version, .. }
            | Self::Command { version, .. }
            | Self::Error { version, .. } => Some(*version),
            Self::Pong => None,
        };
        if version.is_some_and(|version| version != SESSION_REGISTRATION_VERSION) {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "incompatible pdiff session daemon frame version",
            ))
        } else {
            Ok(())
        }
    }
}

pub fn write_session_frame<W: Write, F: SessionWireFrame>(
    writer: &mut W,
    frame: &F,
) -> io::Result<()> {
    frame.validate_version()?;
    let payload = serde_json::to_vec(frame)?;
    if payload.len() > MAX_SESSION_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "pdiff session frame exceeds 1 MiB",
        ));
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

pub fn read_session_frame<R: Read, F: SessionWireFrame>(reader: &mut R) -> io::Result<F> {
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_SESSION_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid bounded pdiff session frame length",
        ));
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    let frame: F = serde_json::from_slice(&payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    frame.validate_version()?;
    Ok(frame)
}
