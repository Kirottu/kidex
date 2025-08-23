use kidex_common::{helper::merge_paths, IndexEntry};
use crate::{index::{GetPath, Index}, ChildIndex};


fn calc_score(query: &str, candidate: &str) -> i32 {
    // TODO: Compare PATHS, because that makes way more sense. Lmao
    let mut score: i32 = -1;

    if candidate.starts_with(query) {
        score += 12 * query.len() as i32
    }
    if candidate.contains(query) {
        score += 6 * query.len() as i32
    }
    score -= query.len().abs_diff(candidate.len()) as i32;

    score
}

// For backend searching. Saves sending the entire index over IPC
pub fn query(index: &Index, query_string: &str) -> Vec<IndexEntry> {
    let mut res: Vec<(i32, IndexEntry)> = index.inner.iter()
        .flat_map(|(desc, dir)| {
            // To build the full path
            let parent_path = index.inner.get_path(desc);
            dir.children.iter().filter_map(move |(path, child)| {
                let mut full_path = merge_paths(&parent_path, path);
                // Algorithm
                let score = calc_score(
                    &query_string.to_lowercase(),
                    &full_path.to_string_lossy().to_lowercase()
                );
                if score <= 0 {
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
