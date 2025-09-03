use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum OutputFormat {
    Json,
    List,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum FileType {
    All,
    FilesOnly,
    DirOnly,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Keyword {
    pub word: String,
    pub exact_match: bool,
}
impl Keyword {
    pub fn new(word: &str, exact_match: bool) -> Self {
        Keyword {
            exact_match,
            word: word.to_lowercase()
                .trim()
                .trim_matches('/')
                .to_string(),
            // TODO: Add case_sensitivity
        }
    }

    pub fn is_in(&self, candidate: &str) -> bool {
        if self.exact_match { candidate.to_lowercase() == self.word }
        else { candidate.to_lowercase().contains(&self.word) }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Query {
    pub file_type: FileType,
    pub direct_parent: Option<Keyword>,
    pub keywords: Vec<Keyword>,
    pub path_keywords: Vec<Keyword>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct QueryOptions {
    pub query: Query,
    pub output_format: OutputFormat,
    pub root_path: Option<PathBuf>,
    pub limit: Option<usize>,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            file_type: FileType::All,
            direct_parent: None,
            keywords: vec![],
            path_keywords: vec![],
        }
    }
}
impl Default for QueryOptions {
    fn default() -> Self {
        QueryOptions {
            output_format: OutputFormat::Json,
            root_path: None,
            query: Query::default(),
            limit: None,
        }
    }
}

impl Query {
    // Takes the elements of the query string
    pub fn from_query_string(s: &str) -> Self {
        Query::from_query_elements(s.split_whitespace().collect())
    }
    pub fn from_query_elements<T: AsRef<str>>(args: Vec<T>) -> Self {
        let mut query = Query::default();
        for arg in args {
            let arg = arg.as_ref();
            let keyword = Keyword::new(arg, arg.ends_with("/"));

            if arg == "/" {
                query.file_type = FileType::DirOnly;
            }
            else if arg == "f/" {
                query.file_type = FileType::FilesOnly;
            }
            else if arg.starts_with("//") {
                query.direct_parent = Some(keyword);
            }
            else if arg.starts_with("/") {
                query.path_keywords.push(keyword);
            }
            else {
                query.keywords.push(keyword);
            }
        }
        
        query
    }
}

pub fn calc_score(query: &Query, path: &Path, is_dir: bool) -> i64 {
    let basename  = path.file_name().unwrap_or_default().to_string_lossy();
    let mut score: i64 = 0;

    // Return immediately when filetype mismatches
    match query.file_type {
        FileType::FilesOnly if is_dir => return -8888,
        FileType::DirOnly if ! is_dir => return -8888,
        _ => (),
    };

    // When set, check if the direct parent of the file matches
    if let Some(parent_dir) = &query.direct_parent {
        let parent_path_name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|p| p.to_str())
            .unwrap_or("");
        if parent_dir.is_in(parent_path_name) {
            score += 1;
        } else {
            return -9999;
        }
    }

    // Check if all the keywords are in the basename
    for kw in &query.keywords {
        score += if ! kw.exact_match && basename.starts_with(&kw.word) {
            50
        } else if kw.is_in(&basename) {
            10
        } else {
            -2000
        }
    }

    // Check if all the path keywords match any of the path components
    for pkw in &query.path_keywords {
        // Check if it's in the path
        let mut in_path = false;
        let mut backdepth = 20;
        for dc in path.components().rev().skip(1) {
            let dir_component = dc.as_os_str().to_string_lossy();
            if pkw.is_in(&dir_component) {
                in_path = true;
                score+=backdepth;
            }
            backdepth -= 4;
        }
        // Penalty if a path_keyword is not in the path at all
        if ! in_path { score -= 5000 }

    }

    score
}

/// Picks the top <limit> elements from a list of scored entries.
/// The top entry will come first
pub fn pick_top_entries<T: Clone>(mut vec: Vec<(i64, T)>, limit: usize) -> Vec<(i64, T)> {
    // If the vec is shorter than the limit, just sort it high to low
    if vec.len() <= limit {
        vec.sort_by_key(|(s, _)| *s);
        vec.reverse();
        return vec;
    }
    /* Note: Make sure this sorting algorithm is 'stable' like the sort_by_key algorithm */
    // Pick the top n most highest ranked entries
    let mut top: Vec<(i64, T)> = Vec::new();
    for (score, entry) in vec {
        // Ignore score if it's worse than the worst one so far
        let worst = top.get(limit-1).map_or(i64::MIN, |f| f.0);
        if score < worst {
            continue;
        }
        let index = top.partition_point(|&(i, _)| i > score);
        top.insert(index, (score, entry.to_owned()));
    }
    // Cut 
    if top.len() > limit {
        top = top.drain(..limit).collect()
    }
    top
}
