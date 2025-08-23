use kidex_common::IndexEntry;
use crate::{index::{GetPath, Index}, ChildIndex};


// You generally want to find one in two
fn calc_score(query: &str, candidate: &str) -> i32 {
    let mut score = -0.1;

    if candidate.starts_with(query) {
        score += 5.0 * query.len() as f32
    }
    if candidate.contains(query) {
        score += 2.0 * query.len() as f32
    }
    score -= (query.len().abs_diff(candidate.len())) as f32 * 0.5;

    score.round() as i32
}

pub fn query(index: &Index, query_string: &str) -> Vec<IndexEntry> {
    let mut res: Vec<(i32, IndexEntry)> = index.inner.iter()
        .flat_map(|(desc, dir)| {
            // To build the full path
            let parent_path = index.inner.get_path(desc);
            dir.children.iter().map(move |(path, child)| {
                let mut full_path = parent_path.clone();
                full_path.push(path);
                // Algorithm
                (
                    calc_score(query_string, &full_path.to_string_lossy()),
                    IndexEntry {
                        path: full_path,
                        directory: matches!(child, ChildIndex::Directory {..}),
                    }
                )
            })
        }).collect();

    res.sort_by_key(|(score, _)| *score);
    res.iter().map(|p| p.1.clone()).collect()
}
