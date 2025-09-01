use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const DEFAULT_SOCKET: &str = "/tmp/kidex.sock";

#[derive(Deserialize, Serialize)]
pub enum IpcCommand {
    FullIndex,
    Quit,
    Reload,
    GetIndex(Option<PathBuf>),
    QueryIndex(QueryOptions),
}

#[derive(Deserialize, Serialize)]
pub enum IpcResponse {
    Success,
    NotFound,
    Index(Vec<IndexEntry>),
}

#[derive(Deserialize, Serialize, Clone)]
pub struct IndexEntry {
    pub path: PathBuf,
    pub directory: bool,
}

#[derive(Deserialize, Serialize, Clone)]
pub enum OutputFormat {
    Json,
    List,
}

#[derive(Deserialize, Serialize, Clone)]
pub enum TypeFilter {
    All,
    FilesOnly,
    DirOnly,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct QueryOptions {
    pub query_string: String,
    pub output_format: OutputFormat,
    pub type_filter: TypeFilter,
    pub root_path: Option<PathBuf>,
}

impl Default for QueryOptions {
    fn default() -> Self {
        QueryOptions {
            query_string: "".to_string(),
            output_format: OutputFormat::Json,
            type_filter: TypeFilter::All,
            root_path: None,
        }
    }
}

impl QueryOptions {
    pub fn from_str(s: &str) -> Self {
        QueryOptions {
            query_string: s.to_string(),
            ..Default::default()
        }
    }
}

pub mod helper {
    use std::path::{Path, PathBuf};
    pub fn merge_paths(path1: &Path, path2: &Path) -> PathBuf {
        return path1.iter().chain(path2.iter()).collect();
    }
}

#[cfg(feature = "util")]
pub mod util {
    use std::{
        env,
        fmt::Display,
        io::{self, Read, Write},
        os::unix::net::UnixStream,
        path::PathBuf,
    };

    use crate::QueryOptions;

    use super::{IndexEntry, IpcCommand, IpcResponse, DEFAULT_SOCKET};

    #[derive(Debug)]
    pub enum Error {
        Io(io::Error),
        Serde(serde_json::Error),
        NotFound,
        Unknown,
    }

    impl From<io::Error> for Error {
        fn from(value: io::Error) -> Self {
            Self::Io(value)
        }
    }

    impl From<serde_json::Error> for Error {
        fn from(value: serde_json::Error) -> Self {
            Self::Serde(value)
        }
    }

    impl Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Error::Io(why) => write!(f, "An IO error occurred: {}", why),
                Error::Serde(why) => write!(f, "A se/deserialization error occurred: {}", why),
                Error::NotFound => write!(f, "Requested path not found"),
                Error::Unknown => write!(f, "An unknown error occurred"),
            }
        }
    }

    impl std::error::Error for Error {}

    fn fetch(command: &IpcCommand) -> Result<IpcResponse, Error> {
        let mut stream =
            UnixStream::connect(env::var("SOCKET_PATH").unwrap_or(DEFAULT_SOCKET.to_string()))?;
        let mut buf = serde_json::to_vec(command).unwrap();
        buf.push(0x0);

        stream.write_all(&buf)?;

        buf.clear();

        stream.read_to_end(&mut buf)?;

        Ok(serde_json::from_slice(&buf)?)
    }

    pub fn query_index(query_opts: QueryOptions) -> Result<Vec<IndexEntry>, Error> {
        match fetch(&IpcCommand::QueryIndex(query_opts))? {
            IpcResponse::Index(index) => Ok(index),
            _ => Err(Error::Unknown),
        }
    }

    pub fn get_index(path: Option<PathBuf>) -> Result<Vec<IndexEntry>, Error> {
        match fetch(&IpcCommand::GetIndex(path))? {
            IpcResponse::Index(index) => Ok(index),
            IpcResponse::NotFound => Err(Error::NotFound),
            _ => unreachable!(),
        }
    }

    pub fn regenerate_index() -> Result<(), Error> {
        match fetch(&IpcCommand::FullIndex)? {
            IpcResponse::Success => Ok(()),
            _ => Err(Error::Unknown),
        }
    }

    pub fn shutdown_server() -> Result<(), Error> {
        match fetch(&IpcCommand::Quit)? {
            IpcResponse::Success => Ok(()),
            _ => Err(Error::Unknown),
        }
    }

    pub fn reload_config() -> Result<(), Error> {
        match fetch(&IpcCommand::Reload)? {
            IpcResponse::Success => Ok(()),
            _ => Err(Error::Unknown),
        }
    }
}
