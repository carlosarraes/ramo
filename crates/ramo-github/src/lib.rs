mod client;
mod error;

pub use client::{GithubClient, GithubViewer};
pub use error::{GithubError, GithubErrorKind};
