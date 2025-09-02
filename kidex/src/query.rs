use std::path::Path;

use kidex_common::{helper::merge_paths, IndexEntry, query::QueryOptions};
use crate::{index::{GetPath, Index}, ChildIndex};


fn calc_score(query: &str, candidate: &Path) -> i32 {
    let mut score: i32 = -1;
    let basename = candidate.file_name().unwrap_or_default().to_string_lossy();
    let _path = candidate.to_string_lossy();

    if basename.starts_with(query) {
        score += 50 * query.len() as i32
    }
    else if basename.contains(query) {
        score += 20 * query.len() as i32
    }

    // Check if it's in the path
    let mut backdepth = 70;
    for dir in candidate.components().rev() {
        if dir.as_os_str()
            .to_string_lossy()
            .contains(query)
        {
            score+=backdepth;
        }
        backdepth -= 10;
        if backdepth <= 0 { break; }
    }

    // score -= query.len().abs_diff(path.len()) as i32;

    score
}

// For backend searching. Saves sending the entire index over IPC
pub fn query(index: &Index, opts: &QueryOptions) -> Vec<IndexEntry> {
    let mut res: Vec<(i32, IndexEntry)> = index.inner.iter()
        .flat_map(|(desc, dir)| {
            // To build the full path
            let parent_path = index.inner.get_path(desc);
            dir.children.iter().filter_map(move |(path, child)| {
                let full_path = merge_paths(&parent_path, path);
                // Algorithm
                // let score = calc_score(
                //     &opts.query,
                //     &full_path
                // );
                let score = 0;
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
