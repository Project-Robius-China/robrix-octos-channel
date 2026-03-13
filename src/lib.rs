//! Bridge domain and transport primitives for connecting Robrix rooms to Crew.
//!
//! This crate intentionally stays UI-agnostic. Robrix should only need to:
//! 1. keep the inventory snapshot in sync with the logged-in Matrix user,
//! 2. edit the policy section through its own UI,
//! 3. resolve a room into an execution profile before opening a Crew session.

pub mod manager;
pub mod resolver;
pub mod state;
pub mod store;
pub mod transport;

pub use manager::{BridgeManager, BridgeManagerError};
pub use resolver::{ResolveError, resolve_room_binding};
pub use state::{
    BridgeDefaults, CrewGlueState, ExecutionProfile, InventorySnapshot, ProviderProfile,
    RoomBinding, RoomInventory, RuntimeState, SessionRecord, SpaceBinding, SpaceInventory,
    StateVersion, UserSnapshot, Workspace,
};
pub use store::{StateStore, StateStoreError};
pub use transport::{
    BridgeError, BridgeEvent, BridgeEventStream, CrewProfileOverride, CrewStreamRequest,
    CrewTransport, SseHttpTransport,
};
