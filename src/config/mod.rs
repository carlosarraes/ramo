mod load;
mod migrate;
mod model;
mod save;

pub(crate) use load::user_config_dir;
pub use load::{ConfigError, ConfigPaths, ConfigResolver};
pub use migrate::{Migration, migrate_user_config};
pub use model::{ConfigLayer, CustomThemeConfig, ResolvedConfig, ThemeSetting, ViewPreferences};
pub use save::{ConfigSaveError, ViewPreferenceChanges, save_view_preferences};
