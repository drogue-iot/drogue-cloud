use crate::state::StateController;
use drogue_cloud_service_api::services::device_state::{DeleteOptions, Id};
use futures::channel::oneshot::{channel, Receiver, Sender};
use std::{
    collections::{hash_map::Entry, HashMap},
    fmt::{Debug, Formatter},
    sync::Arc,
};
use tokio::sync::Mutex;

struct MuxEntry {
    token: String,
    tx: Sender<LostCause>,
}

pub struct Mux {
    handles: HashMap<Id, MuxEntry>,
}

impl Debug for Mux {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mux")
            .field("handles", &self.handles.len())
            .finish()
    }
}

impl Mux {
    pub(crate) fn new() -> Self {
        Self {
            handles: Default::default(),
        }
    }

    /// Handle a lost Id.
    pub(crate) async fn handle_lost(&mut self, id: Id) {
        if let Some(entry) = self.handles.remove(&id) {
            entry.tx.send(LostCause::Reported).ok();
        }
    }

    /// Add a new handle, possibly dropping a conflicting handle.
    pub(crate) fn added(&mut self, id: Id, token: String) -> StateWatcher {
        let (tx, rx) = channel();

        if let Some(old) = self.handles.insert(id, MuxEntry { token, tx }) {
            old.tx.send(LostCause::NewRegistration).ok();
        }

        StateWatcher { rx }
    }

    /// Mark the handle deleted.
    async fn deleted(&mut self, id: Id, token: &str) {
        if let Entry::Occupied(entry) = self.handles.entry(id) {
            if entry.get().token == token {
                entry.remove().tx.send(LostCause::Deleted).ok();
            }
        }
    }
}

pub struct State {
    pub(crate) handle: StateHandle,
    pub(crate) watcher: StateWatcher,
}

pub struct StateWatcher {
    rx: Receiver<LostCause>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LostCause {
    NewRegistration,
    Reported,
    Deleted,
}

impl StateWatcher {
    pub async fn lost(self) -> Option<LostCause> {
        self.rx.await.ok()
    }
}

impl State {
    pub fn split(self) -> (StateHandle, StateWatcher) {
        (self.handle, self.watcher)
    }
}

pub struct StateHandle {
    pub(crate) mux: Arc<Mutex<Mux>>,
    pub(crate) deleted: bool,
    pub(crate) application: String,
    pub(crate) device: String,
    pub(crate) token: String,
    pub(crate) state: StateController,
}

impl StateHandle {
    pub async fn delete(&mut self, opts: DeleteOptions) {
        if self.deleted {
            return;
        }

        self.state
            .delete(&self.application, &self.device, &self.token, opts)
            .await;

        self.mux
            .lock()
            .await
            .deleted(
                Id {
                    application: self.application.to_string(),
                    device: self.device.to_string(),
                },
                &self.token,
            )
            .await;

        self.deleted = true;
    }
}

impl Drop for StateHandle {
    fn drop(&mut self) {
        if !self.deleted {
            let state = self.state.clone();
            let application = self.application.clone();
            let device = self.device.clone();
            let token = self.token.clone();
            let mux = self.mux.clone();
            let id = Id {
                application: self.application.to_string(),
                device: self.device.to_string(),
            };

            tokio::spawn(async move {
                state
                    .delete(&application, &device, &token, Default::default())
                    .await;

                mux.lock().await.deleted(id, &token).await;
            });
        }
    }
}
