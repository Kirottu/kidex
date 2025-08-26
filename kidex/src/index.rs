use std::{
    collections::HashMap,
    ffi::OsStr,
    fs::{self, File},
    io,
    path::PathBuf,
    sync::Arc,
};

use inotify::{Event, Inotify, WatchDescriptor, WatchMask};

use crate::{ChildIndex, Config, DirectoryIndex, WatchDir};

/// The main index struct
pub struct Index {
    pub inner: HashMap<WatchDescriptor, DirectoryIndex>,
    /// The mask used for the watchers
    mask: WatchMask,
}

pub trait GetPath {
    fn get_path(&self, desc: &WatchDescriptor) -> PathBuf;
}

impl GetPath for HashMap<WatchDescriptor, DirectoryIndex> {
    fn get_path(&self, desc: &WatchDescriptor) -> PathBuf {
        let mut desc = Some(desc);

        let mut paths = Vec::new();

        while let Some(new_desc) = desc {
            let dir = match self.get(new_desc) {
                Some(dir) => dir,
                None => {
                    log::warn!("Unknown descriptor used for path!");
                    break;
                }
            };
            paths.push(dir.path.as_path().as_os_str());
            desc = dir.parent.as_ref();
        }

        PathBuf::from(
            paths
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(OsStr::new("/")),
        )
    }
}

impl Index {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            mask: WatchMask::MOVE | WatchMask::CREATE | WatchMask::DELETE,
        }
    }

    /// Index creation for events where a file is "created"
    pub fn create_index(&mut self, inotify: &mut Inotify, path: &PathBuf, event: &Event<&OsStr>) {
        let full_path = self
            .inner
            .get_path(&event.wd)
            .iter()
            .chain(path.iter())
            .collect::<PathBuf>();

        if self
            .inner
            .get(&event.wd)
            .unwrap()
            .watch_dir
            .ignored
            .iter()
            .any(|pat| pat.matches(&full_path.as_os_str().to_string_lossy()))
        {
            return;
        }

        let file = match File::open(full_path) {
            Ok(file) => file,
            Err(why) => {
                log::error!("Failed to open file: {}", why);
                return;
            }
        };
        let child = if file.metadata().unwrap().file_type().is_dir() {
            // If recursion is enabled, recurse through the directories
            if self.inner.get(&event.wd).unwrap().watch_dir.recurse {
                log::info!("Directory created, adding watcher!");
                match self.index_dir(
                    inotify,
                    self.inner.get(&event.wd).unwrap().watch_dir.clone(),
                    path,
                    Some(event.wd.clone()),
                ) {
                    Ok(Some((child, index))) => {
                        self.inner.extend(index.into_iter());
                        child
                    }
                    Ok(None) => return,
                    Err(why) => {
                        log::error!("Failed to index directory: {}", why);
                        return;
                    }
                }
            } else {
                ChildIndex::Directory { descriptor: None }
            }
        } else if file.metadata().unwrap().file_type().is_file() {
            ChildIndex::File {}
        } else {
            log::warn!("A non-file and non-directory created!");
            return;
        };

        self.inner
            .get_mut(&event.wd)
            .unwrap()
            .children
            .insert(path.clone(), child);
    }

    /// Recursively remove indexed directory/file and remove all watchers
    pub fn remove_index(&mut self, inotify: &mut Inotify, path: &PathBuf, event: &Event<&OsStr>) {
        match self.inner.get_mut(&event.wd).unwrap().children.remove(path) {
            Some(child) => {
                if let ChildIndex::Directory {
                    descriptor: Some(descriptor),
                } = child
                {
                    for (desc, dir) in self.traverse(descriptor).into_iter() {
                        log::trace!("Deleted subdir {}", dir.path.display());

                        // Delete current descriptor watcher and delete it from the index
                        assert!(self.inner.remove(&desc).is_some());
                        if let Err(why) = inotify.rm_watch(desc) {
                            log::error!("Failed to remove watcher: {}", why);
                        }
                    }
                }
                assert!(self
                    .inner
                    .get_mut(&event.wd)
                    .unwrap()
                    .children
                    .remove(path)
                    .is_none());
            }
            None => {
                log::warn!(
                    "Non-indexed file {} asked to be un-indexed! Something is probably wrong!",
                    path.display()
                );
            }
        }
    }

    /// Return everything under the selected directory
    pub fn traverse(&self, desc: WatchDescriptor) -> HashMap<WatchDescriptor, DirectoryIndex> {
        let mut queue = vec![desc];

        let mut slice = HashMap::new();

        while !queue.is_empty() {
            let desc = queue.pop().unwrap();
            let dir = self.inner.get(&desc).unwrap();

            // If there are subdirectories, add them to the queue
            queue.extend(
                dir.children
                    .iter()
                    .filter_map(|(_path, child)| match child {
                        ChildIndex::Directory {
                            descriptor: Some(descriptor),
                        } => Some(descriptor.clone()),
                        _ => None,
                    }),
            );

            slice.insert(desc, dir.clone());
        }

        slice
    }

    /// Index everything inside a directory and the directory, and recurse if enabled
    pub fn index_dir(
        &self,
        inotify: &mut Inotify,
        watch_dir: Arc<WatchDir>,
        path: &PathBuf,
        parent: Option<WatchDescriptor>,
    ) -> io::Result<Option<(ChildIndex, HashMap<WatchDescriptor, DirectoryIndex>)>> {
        let full_path = match &parent {
            Some(parent) => {
                let mut new_path = self.inner.get_path(parent);
                new_path.extend(path.iter());
                new_path
            }
            None => path.clone(),
        };

        if watch_dir
            .ignored
            .iter()
            .any(|pat| pat.matches(&path.to_string_lossy()))
        {
            return Ok(None);
        }

        let desc = inotify.add_watch(&full_path, self.mask)?;

        let mut index = HashMap::new();

        index.insert(
            desc.clone(),
            DirectoryIndex {
                path: path.clone(),
                children: HashMap::new(),
                watch_dir: watch_dir.clone(),
                parent,
            },
        );

        let mut queue = fs::read_dir(&full_path)?
            .filter_map(|res| res.ok().map(|entry| (entry, desc.clone())))
            .collect::<Vec<_>>();

        while !queue.is_empty() {
            let (entry, desc) = queue.pop().unwrap();
            let path = entry.path().file_name().map(PathBuf::from).unwrap();

            // Ignore files specified with ignore patterns
            if !watch_dir
                .ignored
                .iter()
                .any(|pat| pat.matches(&path.to_string_lossy()))
            {
                let full_path = index
                    .get_path(&desc)
                    .iter()
                    .chain(path.iter())
                    .collect::<PathBuf>();
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(why) => {
                        log::error!("Failed to determine file type, skipping: {}", why);
                        continue;
                    }
                };

                if file_type.is_dir() && watch_dir.recurse {
                    let new_desc = match inotify.add_watch(&full_path, self.mask) {
                        Ok(new_desc) => {
                            log::trace!("Indexed subdirectory {}", full_path.display());
                            match fs::read_dir(&full_path) {
                                Ok(entries) => queue.extend(entries.filter_map(|res| {
                                    res.ok().map(|entry| (entry, new_desc.clone()))
                                })),
                                Err(why) => {
                                    log::error!(
                                        "Failed to read directory entries, skipping: {}",
                                        why
                                    );
                                    continue;
                                }
                            }
                            index.insert(
                                new_desc.clone(),
                                DirectoryIndex {
                                    path: path.clone(),
                                    children: HashMap::new(),
                                    watch_dir: watch_dir.clone(),
                                    parent: Some(desc.clone()),
                                },
                            );
                            Some(new_desc)
                        }
                        Err(why) => {
                            log::error!(
                                "Failed to create listener for directory, skipping: {}",
                                why
                            );
                            None
                        }
                    };

                    index.get_mut(&desc).unwrap().children.insert(
                        path.clone(),
                        ChildIndex::Directory {
                            descriptor: new_desc,
                        },
                    );
                } else if file_type.is_dir() {
                    index
                        .get_mut(&desc)
                        .unwrap()
                        .children
                        .insert(path, ChildIndex::Directory { descriptor: None });
                } else if file_type.is_file() {
                    index
                        .get_mut(&desc)
                        .unwrap()
                        .children
                        .insert(path, ChildIndex::File {});
                }
            }
        }

        Ok(Some((
            ChildIndex::Directory {
                descriptor: Some(desc),
            },
            index,
        )))
    }

    /// Completely clear and reindex everything
    pub fn full_index(&mut self, inotify: &mut Inotify, config: &Config) -> io::Result<()> {
        log::info!("Starting full index");

        self.clear_index(inotify)?;

        for watch_dir in &config.directories {
            // Extend the WatchDir's ignored list with the global ignored list
            let mut new_watch_dir = watch_dir.clone();
            new_watch_dir.ignored.extend(config.ignored.iter().cloned());

            match self.index_dir(
                inotify,
                Arc::new(new_watch_dir),
                &watch_dir.path,
                None,
            ) {
                Ok(Some((_, index))) => self.inner.extend(index.into_iter()),
                Ok(None) => (),
                Err(why) => {
                    log::error!("Skipping WatchDir {:?} due to error: {}", watch_dir.path, why);
                    continue;
                }
            }
        }

        log::info!("Full index done!");

        Ok(())
    }

    pub fn clear_index(&mut self, inotify: &mut Inotify) -> io::Result<()> {
        // Remove every watcher
        for descriptor in self.inner.keys() {
            inotify.rm_watch(descriptor.clone())?;
        }
        // Clear the inner index after it has been cleaned up
        self.inner.clear();

        Ok(())
    }
}
