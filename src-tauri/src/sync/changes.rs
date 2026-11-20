//! The changes of a plan as a folder tree for the analysis.
//!
//! The rows of a filter (`Plan::matching`) are grouped by their folders: every folder carries what
//! changes below it (number per kind, size), so the tree can show where the changes are without
//! listing them all. Folders come closed: the level of a folder is asked for when it is opened
//! (`tree` with its path). A chain of folders that only hold the next one is shown as one
//! ("docs/2024/raw").

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use super::planner::{Plan, Row};

/// At most this many entries (folders and files) per level; the rest is only counted (`more`).
const LIMIT: usize = 1000;

/// The kinds of rows, in the order of `Tally::kinds`
const KINDS: [&str; 8] = ["copy", "update", "move", "move_dir", "delete", "delete_dir", "create_dir", "blocked"];

/// One level of the tree: folders first, then files, each sorted by name.
#[derive(Debug, Serialize)]
pub struct TreeLevel<'a> {
    pub nodes: Vec<TreeNode<'a>>,
    /// Entries of this level beyond `LIMIT`
    pub more: usize,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TreeNode<'a> {
    Folder {
        /// Relative to the level; several names with "/" for a chain of folders
        name: String,
        /// Relative to the root of the plan, to ask for its level
        path: String,
        /// The change of the folder itself (created, deleted, moved, not transferable)
        #[serde(skip_serializing_if = "Option::is_none")]
        row: Option<Row<'a>>,
        /// Rows below it
        count: usize,
        /// Their size
        bytes: u64,
        /// Rows below it per kind (only kinds that occur)
        kinds: BTreeMap<&'static str, usize>,
    },
    Entry {
        /// File name ("" for the source folder itself)
        name: &'a str,
        row: Row<'a>,
    },
}

#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    kinds: [usize; KINDS.len()],
    bytes: u64,
}

impl Tally {
    fn add(&mut self, row: &Row) {
        let kind = row.kind_name();
        if let Some(i) = KINDS.iter().position(|&k| k == kind) {
            self.kinds[i] += 1;
        }
        self.bytes += row.size();
    }

    fn count(&self) -> usize {
        self.kinds.iter().sum()
    }

    fn by_kind(&self) -> BTreeMap<&'static str, usize> {
        KINDS.iter().zip(self.kinds).filter(|(_, n)| *n > 0).map(|(&k, n)| (k, n)).collect()
    }
}

/// A folder while the tree is built
#[derive(Default)]
struct Dir<'a> {
    row: Option<Row<'a>>,
    dirs: HashMap<&'a str, Dir<'a>>,
    files: Vec<(&'a str, Row<'a>)>,
    /// Rows below it
    tally: Tally,
}

impl<'a> Dir<'a> {
    /// Files below `rel` (relative to this folder) go into their folders; a folder's own row
    /// marks its node. Every folder on the way counts the row.
    fn insert(&mut self, rel: &'a str, row: Row<'a>) {
        let mut dir = self;
        let mut rest = rel;
        loop {
            dir.tally.add(&row);
            match rest.split_once('/') {
                Some((head, tail)) => {
                    dir = dir.dirs.entry(head).or_default();
                    rest = tail;
                }
                None if row.is_folder() && !rest.is_empty() => {
                    dir.dirs.entry(rest).or_default().row = Some(row);
                    return;
                }
                None => {
                    dir.files.push((rest, row));
                    return;
                }
            }
        }
    }

    /// Its folders as shown: sorted by name (case does not matter), chains merged, at most `LIMIT`
    fn shown_dirs(&self) -> Vec<(String, &Dir<'a>)> {
        let mut dirs: Vec<_> = self.dirs.iter().collect();
        dirs.sort_by_cached_key(|(name, _)| (name.to_lowercase(), **name));
        dirs.into_iter().take(LIMIT).map(|(name, dir)| merge_chain(name, dir)).collect()
    }
}

/// A folder that only holds one other folder (and has no change of its own) is shown together with
/// it, and so on down the chain.
fn merge_chain<'b, 'a>(name: &str, mut dir: &'b Dir<'a>) -> (String, &'b Dir<'a>) {
    let mut name = name.to_string();
    while dir.row.is_none() && dir.files.is_empty() && dir.dirs.len() == 1 {
        let (child, next) = dir.dirs.iter().next().expect("one folder");
        name.push('/');
        name.push_str(child);
        dir = next;
    }
    (name, dir)
}

fn join(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.to_string()
    } else {
        format!("{base}/{name}")
    }
}

/// Serializes the level of `dir` (at `base`).
fn level<'a>(dir: &Dir<'a>, base: &str) -> TreeLevel<'a> {
    let mut nodes = Vec::with_capacity((dir.dirs.len() + dir.files.len()).min(LIMIT));
    for (name, sub) in dir.shown_dirs() {
        let path = join(base, &name);
        nodes.push(TreeNode::Folder {
            name,
            path,
            row: sub.row,
            count: sub.tally.count(),
            bytes: sub.tally.bytes,
            kinds: sub.tally.by_kind(),
        });
    }
    let room = LIMIT - nodes.len();
    if room > 0 {
        let mut files: Vec<_> = dir.files.iter().collect();
        files.sort_by_cached_key(|(name, _)| (name.to_lowercase(), *name));
        nodes.extend(files.into_iter().take(room).map(|&(name, row)| TreeNode::Entry { name, row }));
    }
    TreeLevel { more: dir.dirs.len() + dir.files.len() - nodes.len(), nodes }
}

/// The level of the folder `dir` ("" = the root) in the tree of these rows; its folders closed.
fn tree_of<'a>(rows: impl Iterator<Item = Row<'a>>, dir: &str) -> TreeLevel<'a> {
    let mut root = Dir::default();
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    for row in rows {
        if let Some(rel) = row.path().strip_prefix(prefix.as_str()) {
            // The folder itself is a row of the level above
            if dir.is_empty() || !rel.is_empty() {
                root.insert(rel, row);
            }
        }
    }
    level(&root, dir)
}

impl Plan {
    /// The level of the folder `dir` ("" = the root) in the tree of the rows of `filter` whose path
    /// contains `query` (see `Plan::matching`); its folders closed.
    pub fn tree<'a>(&'a self, filter: &'a str, query: &str, dir: &str) -> TreeLevel<'a> {
        tree_of(self.matching(filter, query), dir)
    }

    /// The level of the folder `dir` in the tree of the first `done` operations: what a running
    /// sync has carried out so far, as it runs them in the order of `ops`. It grows with every
    /// operation.
    pub fn done_tree(&self, done: usize, dir: &str) -> TreeLevel<'_> {
        tree_of(self.ops[..done.min(self.ops.len())].iter().map(Row::Op), dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::issues::{Issue, Object, Reason};
    use crate::sync::planner::{Op, OpKind};

    use crate::model::Mode;
    use crate::sync::planner::Summary;
    use crate::sync::tree::ScanMode;

    fn op(kind: OpKind, path: &str, size: u64) -> Op {
        Op::new(kind, path, size)
    }

    fn plan(ops: Vec<Op>, blocked: Vec<Issue>) -> Plan {
        Plan {
            id: "p".into(),
            backup_id: "b".into(),
            archive_id: "a".into(),
            backup_name: "B".into(),
            source: "/src".into(),
            dest: "/archive".into(),
            mode: Mode::Mirror,
            ops,
            summary: Summary::default(),
            warnings: vec![],
            blocked,
            dangerous: false,
            scan_ms: 0,
            diff_ms: 0,
            created_at: chrono::Utc::now(),
            src_scan_ms: 0,
            src_scan: ScanMode::Full,
            dst_scan_ms: 0,
            dst_scan: None,
            src_files: 0,
            src_dirs: 0,
            dst_files: 0,
            dst_dirs: 0,
            probed: true,
            probe_ms: None,
            leftovers: vec![],
        }
    }

    fn names(level: &TreeLevel) -> Vec<String> {
        level
            .nodes
            .iter()
            .map(|n| match n {
                TreeNode::Folder { name, .. } => format!("{name}/"),
                TreeNode::Entry { name, .. } => name.to_string(),
            })
            .collect()
    }

    fn folder<'l, 'a>(level: &'l TreeLevel<'a>, wanted: &str) -> &'l TreeNode<'a> {
        level
            .nodes
            .iter()
            .find(|n| matches!(n, TreeNode::Folder { name, .. } if name == wanted))
            .unwrap_or_else(|| panic!("no folder {wanted} in {:?}", names(level)))
    }

    #[test]
    fn rows_are_grouped_by_folder_with_what_changes_below() {
        let p = plan(
            vec![
                op(OpKind::Delete, "old/x.txt", 5),
                op(OpKind::DeleteDir, "old", 0),
                op(OpKind::CreateDir, "docs", 0),
                op(OpKind::Copy, "docs/b.txt", 20),
                op(OpKind::Copy, "docs/A.txt", 10),
                op(OpKind::Update, "top.txt", 1),
            ],
            vec![],
        );
        let tree = p.tree("all", "", "");
        assert_eq!(names(&tree), ["docs/", "old/", "top.txt"], "folders first, by name");
        let TreeNode::Folder { row, count, bytes, kinds, .. } = folder(&tree, "docs") else { unreachable!() };
        assert_eq!(row.map(|r| r.kind_name()), Some("create_dir"), "the folder's own change");
        assert_eq!((*count, *bytes), (2, 30), "only what lies below it");
        assert_eq!(kinds.get("copy"), Some(&2));
        assert_eq!(names(&p.tree("all", "", "docs")), ["A.txt", "b.txt"], "case does not matter");
        let TreeNode::Folder { kinds, .. } = folder(&tree, "old") else { unreachable!() };
        assert_eq!(kinds.get("delete"), Some(&1));
    }

    #[test]
    fn chains_of_single_folders_are_merged_and_levels_load_on_their_own() {
        let p = plan(vec![op(OpKind::Copy, "a/b/c/one.txt", 1), op(OpKind::Copy, "a/b/c/d/two.txt", 1)], vec![]);
        let tree = p.tree("all", "", "");
        assert_eq!(names(&tree), ["a/b/c/"]);
        let TreeNode::Folder { path, .. } = folder(&tree, "a/b/c") else { unreachable!() };
        assert_eq!(path, "a/b/c");
        let below = p.tree("all", "", "a/b/c");
        assert_eq!(names(&below), ["d/", "one.txt"]);
        let TreeNode::Folder { path, .. } = folder(&below, "d") else { unreachable!() };
        assert_eq!(path, "a/b/c/d", "paths stay relative to the root");
    }

    #[test]
    fn large_levels_are_cut() {
        let ops = (0..LIMIT + 5).map(|i| op(OpKind::Copy, &format!("big/{i:05}.bin"), 1)).collect();
        let p = plan(ops, vec![]);
        let tree = p.tree("all", "", "");
        let TreeNode::Folder { count, .. } = folder(&tree, "big") else { unreachable!() };
        assert_eq!(*count, LIMIT + 5);
        let big = p.tree("all", "", "big");
        assert_eq!((big.nodes.len(), big.more), (LIMIT, 5));
    }

    #[test]
    fn the_tree_of_a_running_sync_holds_only_what_is_done() {
        let p = plan(
            vec![op(OpKind::CreateDir, "docs", 0), op(OpKind::Copy, "docs/a.txt", 3), op(OpKind::Copy, "top.txt", 1)],
            vec![Issue::new("", Object::Folder, Reason::AccessDenied)],
        );
        assert!(p.done_tree(0, "").nodes.is_empty(), "nothing done yet, and no blocked entries");
        let tree = p.done_tree(2, "");
        assert_eq!(names(&tree), ["docs/"]);
        let TreeNode::Folder { count, bytes, .. } = folder(&tree, "docs") else { unreachable!() };
        assert_eq!((*count, *bytes), (1, 3));
        assert_eq!(names(&p.done_tree(99, "")), ["docs/", "top.txt"], "more than there are: all");
    }

    #[test]
    fn filter_and_search_decide_what_the_tree_holds() {
        let p = plan(
            vec![op(OpKind::Copy, "docs/report.pdf", 1), op(OpKind::Delete, "docs/old.pdf", 1)],
            vec![Issue::new("", Object::Folder, Reason::AccessDenied)],
        );
        assert_eq!(names(&p.tree("all", "", "")), ["docs/", ""], "the source folder itself is an entry");
        let deleted = p.tree("delete", "", "");
        assert_eq!(names(&deleted), ["docs/"]);
        let TreeNode::Folder { kinds, .. } = folder(&deleted, "docs") else { unreachable!() };
        assert_eq!(kinds.keys().collect::<Vec<_>>(), [&"delete"]);
        let found = p.tree("all", "REPORT", "");
        let TreeNode::Folder { count, .. } = folder(&found, "docs") else { unreachable!() };
        assert_eq!(*count, 1);
    }
}
