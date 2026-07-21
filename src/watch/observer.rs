use std::sync::mpsc::{self, Receiver};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use super::{WatchPlan, WatchTarget};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ObserverPoll {
    pub changed: bool,
    pub error: Option<String>,
}

pub struct NativeObserver {
    _watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<Event>>,
    targets: Vec<WatchTarget>,
}

impl NativeObserver {
    pub fn start(plan: &WatchPlan) -> Result<Self, notify::Error> {
        let (sender, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })?;
        for target in &plan.targets {
            match target {
                WatchTarget::Entries { directory, .. } => {
                    watcher.watch(directory, RecursiveMode::NonRecursive)?;
                }
                WatchTarget::Tree { directory } => {
                    watcher.watch(directory, RecursiveMode::Recursive)?;
                }
            }
        }
        Ok(Self {
            _watcher: watcher,
            receiver,
            targets: plan.targets.clone(),
        })
    }

    pub fn poll(&mut self) -> ObserverPoll {
        let mut result = ObserverPoll::default();
        for event in self.receiver.try_iter() {
            match event {
                Ok(event) => {
                    result.changed |= event.paths.iter().any(|path| self.matches(path));
                }
                Err(error) => result.error = Some(error.to_string()),
            }
        }
        result
    }

    fn matches(&self, path: &std::path::Path) -> bool {
        self.targets.iter().any(|target| match target {
            WatchTarget::Entries { directory, entries } => {
                path == directory || entries.iter().any(|entry| entry == path)
            }
            WatchTarget::Tree { directory } => path.starts_with(directory),
        })
    }
}
