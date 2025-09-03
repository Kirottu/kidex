use kidex_common::{helper::merge_paths, query::{calc_score, QueryOptions}, IndexEntry};
use crate::{index::{GetPath, Index}, ChildIndex};

// For backend searching. Saves sending the entire index over IPC
pub fn query(index: &Index, opts: &QueryOptions) -> Vec<IndexEntry> {
    let mut res: Vec<(i64, IndexEntry)> = index.inner.iter()
        .flat_map(|(desc, dir)| {
            // To build the full path
            let parent_path = index.inner.get_path(desc);
            dir.children.iter().filter_map(move |(path, child)| {
                let full_path = merge_paths(&parent_path, path);
                let score = calc_score(
                    &opts.query,
                    &full_path,
                    matches!(child, ChildIndex::Directory {..}),
                );
                if score >= 0 {
                    Some(
                        (score,
                         IndexEntry {
                            path: full_path,
                            directory: matches!(child, ChildIndex::Directory {..}),
                        })
                    )
                } else {
                    None
                }
            })
        })
    .collect();

    res.sort_by_key(|(score, _)| *score);
    res.iter().map(|p| p.1.clone()).collect()
}
