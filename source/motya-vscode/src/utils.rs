use std::path::{Path, PathBuf};
use path_clean::PathClean;

pub fn normalize_path(base: &Path, relative: &str) -> PathBuf {
    base.join(relative).clean()
}