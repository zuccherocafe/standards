//! Pure import-path helpers; filesystem discovery is outside this fixture.

use std::path::Path;

pub(crate) fn is_soppo_import(_path: &str, _module: &str, _root: &Path) -> bool {
    false
}

pub(crate) fn get_local_package_path<'a>(path: &'a str, module: &str) -> Option<&'a str> {
    path.strip_prefix(module)?.strip_prefix('/')
}
