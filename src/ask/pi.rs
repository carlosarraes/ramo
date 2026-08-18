//! Ask's view of the shared pi CLI wrapper.
//!
//! The wrapper itself lives in `ramo_core::pi` so `ramo-server` can use it for the Review Map
//! without depending on the terminal crate. Ask keeps its own names and, crucially, its own
//! guarantee: `PiTools::None` plus `PiSession::Ephemeral` is what renders as
//! `--no-tools --no-session`.

pub use ramo_core::pi::{
    PI_STDERR_LIMIT, PI_STDOUT_LIMIT, PiCli, PiError as AskError, PiRequest as AskRequest,
    PiSession, PiTools,
};
