mod client;
mod error;
mod inbox;

pub use client::{GithubClient, GithubViewer};
pub use error::{GithubError, GithubErrorKind};
