use ramo::cli::Action;
use ramo::core::input::{CommonOptions, ReviewInput};
use ramo::runtime::{
    StartupAction, resolve_action, should_finish_local_annotations, stdin_needs_tty_replacement,
};

#[test]
fn integrations_do_not_initialize_the_review_ui() {
    assert_eq!(resolve_action(&Action::InstallPi), StartupAction::InstallPi);
    assert_eq!(
        resolve_action(&Action::UninstallPi),
        StartupAction::UninstallPi
    );
}

#[test]
fn printable_output_does_not_initialize_the_review_ui() {
    assert_eq!(
        resolve_action(&Action::Print("help".into())),
        StartupAction::Print
    );
}

#[test]
fn only_piped_stdin_needs_a_tty_replacement() {
    assert!(stdin_needs_tty_replacement(false));
    assert!(!stdin_needs_tty_replacement(true));
}

#[test]
fn remote_reviews_never_fall_through_to_local_markdown_export() {
    let input = ReviewInput::PullRequest {
        number: 123,
        options: CommonOptions::default(),
    };
    assert!(!should_finish_local_annotations(&input, None));
    assert!(!should_finish_local_annotations(
        &input,
        Some(ramo::app::RemoteReviewOutcome::Published)
    ));
}
