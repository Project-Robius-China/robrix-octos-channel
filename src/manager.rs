use crate::resolver::{ResolveError, ResolvedBinding, resolve_room_binding};
use crate::state::{
    CrewGlueState, InventorySnapshot, ProviderProfile, RuntimeState, SessionRecord, UserSnapshot,
};
use crate::store::{StateStore, StateStoreError};
use crate::transport::{BridgeError, CrewStreamRequest, SseHttpTransport};
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum BridgeManagerError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Store(#[from] StateStoreError),
    #[error(transparent)]
    Transport(#[from] BridgeError),
    #[error("provider '{provider_id}' has no Crew base url configured")]
    MissingBaseUrl { provider_id: String },
    #[error("provider '{provider_id}' has invalid Crew base url '{base_url}'")]
    InvalidBaseUrl {
        provider_id: String,
        base_url: String,
        #[source]
        error: url::ParseError,
    },
}

/// High-level facade used by Robrix.
///
/// This keeps the file-backed state and the transport together so Robrix can
/// treat the crate as a single service boundary.
pub struct BridgeManager {
    store: StateStore,
    state: CrewGlueState,
}

impl BridgeManager {
    pub fn from_parts(store: StateStore, state: CrewGlueState) -> Self {
        Self { store, state }
    }

    pub fn state(&self) -> &CrewGlueState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut CrewGlueState {
        &mut self.state
    }

    pub fn store(&self) -> &StateStore {
        &self.store
    }

    pub fn refresh_inventory(&mut self, user: UserSnapshot, inventory: InventorySnapshot) {
        self.state.refresh_inventory(user, inventory);
    }

    pub fn resolve_room(&self, room_id: &str) -> Result<ResolvedBinding, ResolveError> {
        resolve_room_binding(&self.state, room_id)
    }

    pub fn save(&self) -> Result<(), StateStoreError> {
        self.store.save(&self.state)
    }

    pub fn prepare_room_message(
        &mut self,
        room_id: &str,
        message: impl Into<String>,
    ) -> Result<(ResolvedBinding, SseHttpTransport, CrewStreamRequest), BridgeManagerError> {
        let (resolved, transport) = self.transport_for_room(room_id)?;
        let session_id = ensure_session_id(&mut self.state.runtime, room_id, &resolved);
        let request = CrewStreamRequest {
            session_id,
            message: message.into(),
        };
        Ok((resolved, transport, request))
    }

    pub fn transport_for_room(
        &self,
        room_id: &str,
    ) -> Result<(ResolvedBinding, SseHttpTransport), BridgeManagerError> {
        let resolved = self.resolve_room(room_id)?;
        let transport = build_transport(&resolved.provider)?;
        Ok((resolved, transport))
    }
}

fn ensure_session_id(
    runtime: &mut RuntimeState,
    room_id: &str,
    resolved: &ResolvedBinding,
) -> String {
    let record = runtime
        .active_sessions
        .entry(room_id.to_string())
        .or_insert_with(|| SessionRecord {
            room_id: room_id.to_string(),
            execution_profile_id: resolved.execution_profile.id.clone(),
            session_id: make_session_id(room_id, &resolved.execution_profile.id),
        });

    if record.execution_profile_id != resolved.execution_profile.id {
        record.execution_profile_id = resolved.execution_profile.id.clone();
        record.session_id = make_session_id(room_id, &resolved.execution_profile.id);
    }

    record.session_id.clone()
}

fn make_session_id(room_id: &str, execution_profile_id: &str) -> String {
    format!("robrix:{room_id}:{execution_profile_id}")
}

fn build_transport(provider: &ProviderProfile) -> Result<SseHttpTransport, BridgeManagerError> {
    let Some(base_url) = provider.base_url.as_deref() else {
        return Err(BridgeManagerError::MissingBaseUrl {
            provider_id: provider.id.clone(),
        });
    };
    let url = Url::parse(base_url).map_err(|error| BridgeManagerError::InvalidBaseUrl {
        provider_id: provider.id.clone(),
        base_url: base_url.to_string(),
        error,
    })?;

    let mut transport = SseHttpTransport::new(url);
    if let Some(env_var) = provider.api_key_env.as_deref()
        && let Ok(token) = std::env::var(env_var)
        && !token.is_empty()
    {
        transport = transport.with_auth_token(token);
    }
    Ok(transport)
}
