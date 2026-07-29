use ramo::cli::Action;
use ramo::core::input::{CommonOptions, InputKind, ReviewInput};
use ramo::runtime::{
    StartupAction, companion_path, resolve_action, should_finish_local_annotations,
    stdin_needs_tty_replacement,
};

#[test]
fn pull_requests_start_on_map_but_local_diffs_and_pagers_start_on_review() {
    assert_eq!(
        ramo::runtime::initial_screen(InputKind::PullRequest, false),
        ramo::app::AppScreen::ReviewMap
    );
    assert_eq!(
        ramo::runtime::initial_screen(InputKind::Diff, false),
        ramo::app::AppScreen::Review
    );
    assert_eq!(
        ramo::runtime::initial_screen(InputKind::PullRequest, true),
        ramo::app::AppScreen::Review
    );
}

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
        with_comments: false,
        options: CommonOptions::default(),
    };
    assert!(!should_finish_local_annotations(&input, None));
    assert!(!should_finish_local_annotations(
        &input,
        Some(ramo::app::RemoteReviewOutcome::Published)
    ));
}

#[test]
fn companion_is_resolved_beside_the_current_ramo_binary() {
    assert_eq!(
        companion_path(std::path::Path::new("/opt/ramo/ramo")),
        std::path::Path::new("/opt/ramo/ramo-server")
    );
    assert_eq!(
        companion_path(std::path::Path::new("/opt/ramo/ramo.exe")),
        std::path::Path::new("/opt/ramo/ramo-server.exe")
    );
}
