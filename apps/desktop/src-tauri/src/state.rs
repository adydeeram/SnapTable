use std::sync::Arc;

use snaptable_drivers::ConnectionManager;
use snaptable_secrets::SecretStore;
use snaptable_store::Store;

pub struct AppState {
    pub store: Store,
    pub secrets: Arc<dyn SecretStore>,
    pub connections: ConnectionManager,
}

impl AppState {
    pub fn new(store: Store, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            store,
            secrets,
            connections: ConnectionManager::default(),
        }
    }
}
