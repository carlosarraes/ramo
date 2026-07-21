mod load;
mod model;
mod save;

pub use load::{ConfigError, ConfigPaths, ConfigResolver};
pub(crate) use load::user_config_dir;
pub use model::{ConfigLayer, CustomThemeConfig, ResolvedConfig, ViewPreferences};
pub use save::{ConfigSaveError, ViewPreferenceChanges, save_view_preferences};
