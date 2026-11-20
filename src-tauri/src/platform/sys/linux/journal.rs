//! Linux has no change journal of its file systems (see `platform::journal`).

use std::collections::HashSet;
use std::path::Path;

use crate::platform::journal::Position;

pub fn position(_root: &Path) -> Option<Position> {
    None
}

pub fn changed_folders(_root: &Path, _since: Position) -> Option<HashSet<u64>> {
    None
}
