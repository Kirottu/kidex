use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const DEFAULT_SOCKET: &str = "/tmp/kidex.sock";

#[derive(Deserialize, Serialize)]
pub enum IpcCommand {
    FullIndex,
    Quit,
    Reload,
    GetIndex(Option<PathBuf>),
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

#[cfg(feature = "util")]
pub mod util {
    use std::{
        env,
        fmt::Display,
        io::{self, Read, Write},
        os::unix::net::UnixStream,
        path::PathBuf,
    };

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
