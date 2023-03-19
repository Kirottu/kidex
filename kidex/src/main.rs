use std::{collections::HashMap, env, fs, io, path::PathBuf, sync::Arc, time::Duration};

use futures::StreamExt;
use globber::Pattern;
use index::{GetPath, Index};
use inotify::{EventMask, Inotify, WatchDescriptor};
use kidex_common::{IndexEntry, IpcCommand, IpcResponse, DEFAULT_SOCKET};
use serde::{de::Error, Deserialize, Deserializer};
use signal_hook::consts::TERM_SIGNALS;
use signal_hook_tokio::Signals;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufStream},
    net::UnixListener,
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex,
    },
};

mod index;

#[derive(Deserialize)]
pub struct Config {
    directories: Vec<WatchDir>,
    #[serde(deserialize_with = "parse_pattern_vec")]
    ignored: Vec<Pattern>,
}

/// Custom parser to handle the patterns
fn parse_pattern_vec<'de, D>(deserializer: D) -> Result<Vec<Pattern>, D::Error>
where
    D: Deserializer<'de>,
{
    let vec = Vec::<String>::deserialize(deserializer)?;
    let mut final_vec = Vec::new();

    for string in vec {
        final_vec.push(match Pattern::new(&string) {
            Ok(pattern) => pattern,
            Err(why) => {
                return Err(D::Error::custom(why));
            }
        });
    }

    Ok(final_vec)
}

/// Describes a directory that is watched for changes
#[derive(Clone, Debug, Deserialize)]
pub struct WatchDir {
    /// Path of the directory
    path: String,
    /// Ignored patterns
    #[serde(deserialize_with = "parse_pattern_vec")]
    ignored: Vec<Pattern>,
    /// Recursively watch directories
    recurse: bool,
}

/// A "top-level" object representing a directory being watched, and keeping track of it's children
#[derive(Debug, Clone)]
pub struct DirectoryIndex {
    path: PathBuf,
    children: HashMap<PathBuf, ChildIndex>,
    /// Reference to the underlying `WatchDir` object containing some
    /// configuration details
    watch_dir: Arc<WatchDir>,
    parent: Option<WatchDescriptor>,
}

/// A child of an indexed directory
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum ChildIndex {
    File {},
    Directory { descriptor: Option<WatchDescriptor> },
}

/// Sent from the IPC listener to the main event loop
#[derive(Debug)]
enum EventLoopMsg {
    FullIndex,
    Quit,
    Reload,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let config_path = format!(
        "{}/.config/kidex.ron",
        match env::var("HOME") {
            Ok(home) => home,
            Err(why) => {
                log::error!("Failed to determine home directory: {}", why);
                return;
            }
        }
    );
    let mut inotify = Inotify::init().expect("Failed to init inotify");
    let mut config: Config = ron::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let mut index = Index::new();

    index
        .full_index(&mut inotify, &config)
        .expect("Failed to complete initial index!");

    let index = Arc::new(Mutex::new(index));

    let socket_path = env::var("SOCKET_PATH").unwrap_or(DEFAULT_SOCKET.to_string());
    // Delete the socket file if it is lingering around
    let _ = fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path).expect("Failed to create unix socket listener");

    // Create necessary communication channels
    let (ipc_tx, mut events_rx) = mpsc::channel::<EventLoopMsg>(32);
    let (events_tx, ipc_rx) = mpsc::channel::<()>(32);

    // Spawn task listening for termination signals
    tokio::spawn(signal_task(
        ipc_tx.clone(),
        Signals::new(TERM_SIGNALS).unwrap(),
    ));
    // Spawn IPC task
    tokio::spawn(ipc_task(listener, index.clone(), ipc_tx, ipc_rx));

    // Buffer used by inotify
    let mut buffer = [0; 1024];

    'event_loop: loop {
        // Sleep for a while to not keep the thread busy all the time
        tokio::time::sleep(Duration::from_millis(10)).await;
        match events_rx.try_recv() {
            Ok(event) => match event {
                EventLoopMsg::FullIndex => {
                    index
                        .lock()
                        .await
                        .full_index(&mut inotify, &config)
                        .unwrap();
                }
                EventLoopMsg::Quit => break,
                EventLoopMsg::Reload => {
                    match serde_json::from_str::<Config>(&fs::read_to_string(&config_path).unwrap())
                    {
                        Ok(new_config) => {
                            config = new_config;
                            // Reindex everything if the config was reloaded
                            index
                                .lock()
                                .await
                                .full_index(&mut inotify, &config)
                                .unwrap();
                        }
                        Err(why) => {
                            log::error!("Failed to load config: {}", why);
                        }
                    }
                }
            },
            Err(mpsc::error::TryRecvError::Empty) => (),
            Err(why) => {
                log::error!("Failed receive message from IPC task: {}", why);
            }
        }

        let events = match inotify.read_events(&mut buffer) {
            Ok(events) => events,
            // The next event(s) is/are not ready yet if the error is WouldBlock
            // so it counterintuitively is not actually an error.
            Err(why) => {
                if why.kind() != io::ErrorKind::WouldBlock {
                    log::error!("Error reading inotify events: {}", why);
                }
                continue 'event_loop;
            }
        };

        for event in events {
            let mut index = index.lock().await;

            if index.inner.get(&event.wd).is_none() {
                log::warn!("Event received from nonexistent watcher: {:?}", event.name);
                continue 'event_loop;
            }

            let path = if let Some(name) = event.name {
                PathBuf::from(name)
            } else {
                log::warn!("Event received with no name!");
                continue 'event_loop;
            };

            let path_str = format!(
                "{}/{}",
                index.inner.get_path(&event.wd).display(),
                path.display()
            );

            if event.mask.contains(EventMask::CREATE) {
                log::info!("File created: {}", path_str);
                index.create_index(&mut inotify, &path, &event);
            }
            if event.mask.contains(EventMask::DELETE) {
                log::info!("File deleted: {}", path_str);
                index.remove_index(&mut inotify, &path, &event);
            }
            if event.mask.contains(EventMask::MOVED_FROM) {
                log::info!("File moved from: {}", path_str);
                index.remove_index(&mut inotify, &path, &event);
            }
            if event.mask.contains(EventMask::MOVED_TO) {
                log::info!("File moved to: {}", path_str);
                index.create_index(&mut inotify, &path, &event);
            }
        }
    }

    index.lock().await.clear_index(&mut inotify).unwrap();

    events_tx.send(()).await.unwrap();
}

async fn signal_task(signal_tx: Sender<EventLoopMsg>, mut signals: Signals) {
    // Wait for a signal to arrive, we only listen for termination signals so any
    // received event will be one we should act on
    signals.next().await;

    log::info!("Termination signal received! Quitting...");

    signal_tx.send(EventLoopMsg::Quit).await.unwrap();
}

async fn ipc_task(
    listener: UnixListener,
    index: Arc<Mutex<Index>>,
    ipc_tx: Sender<EventLoopMsg>,
    mut ipc_rx: Receiver<()>,
) {
    loop {
        tokio::select! {
            Ok((stream, _)) = listener.accept() => {
                let mut buf = Vec::new();
                let mut stream = BufStream::new(stream);
                stream.read_until(0x0, &mut buf).await.unwrap();
                buf.pop(); // Remove the delimiting null byte

                match serde_json::from_slice::<IpcCommand>(&buf).unwrap() {
                    IpcCommand::FullIndex => {
                        ipc_tx.send(EventLoopMsg::FullIndex).await.unwrap();
                        if let Err(why) = stream.write_all(&serde_json::to_vec(&IpcResponse::Success).unwrap()).await {
                            log::error!("Error writing reply to stream: {}", why);
                        }
                    }
                    IpcCommand::Quit => {
                        ipc_tx.send(EventLoopMsg::Quit).await.unwrap();
                        if let Err(why) = stream.write_all(&serde_json::to_vec(&IpcResponse::Success).unwrap()).await {
                            log::error!("Error writing reply to stream: {}", why);
                        }
                        break;
                    }
                    IpcCommand::Reload => {
                        ipc_tx.send(EventLoopMsg::Reload).await.unwrap();
                        if let Err(why) = stream.write_all(&serde_json::to_vec(&IpcResponse::Success).unwrap()).await {
                            log::error!("Error writing reply to stream: {}", why);
                        }
                    }
                    IpcCommand::GetIndex(path) => {
                        let index = index.lock().await;
                        let paths = match path {
                            Some(path) => {
                                index
                                    .inner
                                    .iter()
                                    .find(|(_, dir)| dir.path == path)
                                    .map(|(desc, _)| index.traverse(desc.clone())
                                        .into_iter()
                                        .flat_map(|(desc, dir)| {
                                            let parent_path = index.inner.get_path(&desc);
                                            dir.children.into_iter().map(move |(path, child)|
                                                IndexEntry {
                                                    path: parent_path.iter().chain(path.iter()).collect(),
                                                    directory: matches!(child, ChildIndex::Directory {..})
                                                }
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                    )
                            }
                            None => Some(index
                                .inner
                                .iter()
                                .flat_map(|(_, dir)| dir.children.iter().map(|(path, child)|
                                    IndexEntry { path: path.clone(), directory: matches!(child, ChildIndex::Directory {..}) }
                                ))
                                .collect()
                            )
                        };

                        let buf = serde_json::to_vec(&match paths {
                            Some(paths) => IpcResponse::Index(paths),
                            None => IpcResponse::NotFound,
                        }).unwrap();

                        stream.write_all(&buf).await.unwrap();
                    },
                }
                stream.flush().await.unwrap();
            }
            _ = ipc_rx.recv() => break
        }
    }
}
