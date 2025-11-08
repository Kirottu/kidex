use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum CaseOption {
    Match,
    Ignore,
    Smart,
}

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
            word: word
                .trim()
                .trim_matches('/')
                .to_string(),
        }
    }

    pub fn is_at_beginning(&self, candidate: &str, case_options: &CaseOption) -> bool {
        match case_options {
            CaseOption::Match => candidate.starts_with(&self.word),
            CaseOption::Ignore => candidate.to_lowercase().starts_with(&self.word.to_lowercase()),
            CaseOption::Smart => {
                if &self.word.to_lowercase() != &self.word {
                    // Case sensitive
                    candidate.starts_with(&self.word)
                } else {
                    // Ignoring case
                    candidate.to_lowercase().starts_with(&self.word.to_lowercase())
                }
            }
        }
    }

    pub fn is_in(&self, candidate: &str, case_options: &CaseOption) -> bool {
        let (cased_candidate, cased_word) = match case_options {
            CaseOption::Match => (candidate.into(), &self.word),
            CaseOption::Ignore => (candidate.to_lowercase(), &self.word.to_lowercase()),
            CaseOption::Smart => {
                if &self.word.to_lowercase() != &self.word {
                    // Case sensitive
                    (candidate.into(), &self.word)
                } else {
                    // Ignoring case
                    (candidate.to_lowercase(), &self.word.to_lowercase())
                }
            },
        };
        if self.exact_match {
            cased_candidate == *cased_word
        } else {
            cased_candidate.contains(cased_word)
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum QueryParameter {
    // Filters by a specific filetype, like directories-only
    Type(FileType),
    // Matching the basename
    Keyword(Keyword),
    // Matching any path element
    PathKeyword(Keyword),
    // Matching only the direct parent directory
    DirectParent(Keyword),
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Query {
    parameters: Vec<QueryParameter>,
    pub case_option: CaseOption,

}

impl Default for Query {
    fn default() -> Self {
        Query{
            parameters: vec![],
            case_option: CaseOption::Smart,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct QueryOptions {
    pub query: Query,
    pub output_format: OutputFormat,
    pub root_path: Option<PathBuf>,
    pub limit: Option<usize>,
}
impl Default for QueryOptions {
    fn default() -> Self {
        QueryOptions {
            query: Query::default(),
            output_format: OutputFormat::Json,
            root_path: None,
            limit: None,
        }
    }
}

impl QueryParameter {
    /// Parses a string into a query parameter
    /// Syntax:
    /// <word>   : match the word in the basename
    /// /<word>  : match the word in the full path, but not the basename
    /// //<word> : match the word in the direct parent directory
    /// <word>/  : match a keyword exactly and not just partly
    ///
    pub fn from_str(s: &str) -> QueryParameter {
        let keyword = Keyword::new(s, s.ends_with("/"));
        if s == "/" {
            return QueryParameter::Type(FileType::DirOnly);
        }
        else if s == "f/" {
            return QueryParameter::Type(FileType::FilesOnly);
        }
        else if s.starts_with("//") {
            return QueryParameter::DirectParent(keyword);
        }
        else if s.starts_with("/") {
            return QueryParameter::PathKeyword(keyword);
        }
        else {
            return QueryParameter::Keyword(keyword);
        }
    }
}

impl Query {
     
    /// Appends a parameter to the Query. 
    /// If a parameter is of any of the following types, it replaces previous parameters of that type:
    /// - [`QueryParameter::Type`]
    pub fn add_parameter(&mut self, param: QueryParameter) {
        // Replace previous type parameters
        if matches!(param, QueryParameter::Type(_)) {
            self.parameters.retain(|p| { ! matches!(p, QueryParameter::Type(_)) });
        };
        self.parameters.push(param);
    }

    /// Applies a Query to a path candidate to calculate a score.
    pub fn calc_score(&self, path: &Path, is_dir: bool) -> i64 {
        let basename  = path.file_name().unwrap_or_default().to_string_lossy();
        let mut score: i64 = 0;

        for param in &self.parameters {
            match param {
                QueryParameter::Type(file_type) => {
                    // Eliminate when filetype mismatches
                    match file_type {
                        FileType::FilesOnly if is_dir => return -8888,
                        FileType::DirOnly if ! is_dir => return -8888,
                        _ => (),
                    };
                },
                QueryParameter::Keyword(keyword) => {
                    // Check if all the keywords are in the basename
                    score += if ! keyword.exact_match && keyword.is_at_beginning(&basename, &self.case_option) {
                        50
                    } else if keyword.is_in(&basename, &self.case_option) {
                        10
                    } else {
                        // Eliminate if a keyword misses in the basename
                        return -2222
                    }
                },
                QueryParameter::PathKeyword(keyword) =>{
                    // Check if all the path keywords match any of the path components
                    let mut in_path = false;
                    let mut backdepth = 20;
                    // Check if a path keyword matches any of the path components
                    // Deeper directories give greater score
                    for dc in path.components().rev().skip(1) {
                        let dir_component = dc.as_os_str().to_string_lossy();
                        if keyword.is_in(&dir_component, &self.case_option) {
                            in_path = true;
                            score+=backdepth;
                        }
                        backdepth -= 4;
                    }
                    // Eliminate if a path_keyword isn't in the path at all
                    if ! in_path { return -5555 }
                },
                QueryParameter::DirectParent(keyword) => {
                    // When set, check if the direct parent of the file matches
                    let parent_path_name = path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|p| p.to_str())
                        .unwrap_or("");
                    if keyword.is_in(parent_path_name, &self.case_option) {
                        score += 1;
                    } else {
                        // Eliminate if parent directory does not match
                        return -9999;
                    }
                },
            }
        }

        score
    }
}


