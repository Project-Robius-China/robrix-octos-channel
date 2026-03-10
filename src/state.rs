use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const STATE_VERSION: u32 = 1;

pub type StateVersion = u32;
pub type ProviderId = String;
pub type WorkspaceId = String;
pub type ExecutionProfileId = String;
pub type RoomId = String;
pub type SpaceId = String;

/// Persistent state for the Robrix <-> Crew bridge.
///
/// `inventory` is refreshed by Robrix from live Matrix state.
/// `policy` remains user-controlled and must not be overwritten by login refreshes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrewGlueState {
    #[serde(default = "default_state_version")]
    pub version: StateVersion,
    #[serde(default)]
    pub user: UserSnapshot,
    #[serde(default)]
    pub inventory: InventorySnapshot,
    #[serde(default)]
    pub providers: BTreeMap<ProviderId, ProviderProfile>,
    #[serde(default)]
    pub workspaces: BTreeMap<WorkspaceId, Workspace>,
    #[serde(default)]
    pub execution_profiles: BTreeMap<ExecutionProfileId, ExecutionProfile>,
    #[serde(default)]
    pub space_bindings: BTreeMap<SpaceId, SpaceBinding>,
    #[serde(default)]
    pub room_bindings: BTreeMap<RoomId, RoomBinding>,
    #[serde(default)]
    pub defaults: BridgeDefaults,
    #[serde(default)]
    pub runtime: RuntimeState,
}

impl Default for CrewGlueState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            user: UserSnapshot::default(),
            inventory: InventorySnapshot::default(),
            providers: BTreeMap::new(),
            workspaces: BTreeMap::new(),
            execution_profiles: BTreeMap::new(),
            space_bindings: BTreeMap::new(),
            room_bindings: BTreeMap::new(),
            defaults: BridgeDefaults::default(),
            runtime: RuntimeState::default(),
        }
    }
}

impl CrewGlueState {
    /// Update the user + discovered Matrix inventory without touching user policy.
    pub fn refresh_inventory(&mut self, user: UserSnapshot, inventory: InventorySnapshot) {
        self.user = user;
        self.inventory = inventory;
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserSnapshot {
    pub matrix_user_id: Option<String>,
    pub homeserver_url: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventorySnapshot {
    #[serde(default)]
    pub rooms: BTreeMap<RoomId, RoomInventory>,
    #[serde(default)]
    pub spaces: BTreeMap<SpaceId, SpaceInventory>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomInventory {
    pub room_id: RoomId,
    pub display_name: Option<String>,
    pub canonical_alias: Option<String>,
    #[serde(default)]
    pub space_ids: Vec<SpaceId>,
    #[serde(default)]
    pub is_direct: bool,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpaceInventory {
    pub space_id: SpaceId,
    pub display_name: Option<String>,
    pub canonical_alias: Option<String>,
    #[serde(default)]
    pub child_room_ids: Vec<RoomId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderProfile {
    pub id: ProviderId,
    pub provider: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_type: Option<String>,
    pub api_key_env: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub root_dir: PathBuf,
    pub data_dir: Option<PathBuf>,
    #[serde(default)]
    pub skills_dirs: Vec<PathBuf>,
    pub description: Option<String>,
}

/// User-facing execution target that a room/space binding points to.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionProfile {
    pub id: ExecutionProfileId,
    pub name: String,
    pub provider_id: ProviderId,
    pub workspace_id: Option<WorkspaceId>,
    pub system_prompt: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpaceBinding {
    pub space_id: SpaceId,
    pub execution_profile_id: ExecutionProfileId,
    #[serde(default = "default_true")]
    pub apply_to_children: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for SpaceBinding {
    fn default() -> Self {
        Self {
            space_id: String::new(),
            execution_profile_id: String::new(),
            apply_to_children: true,
            enabled: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomBinding {
    pub room_id: RoomId,
    pub execution_profile_id: ExecutionProfileId,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for RoomBinding {
    fn default() -> Self {
        Self {
            room_id: String::new(),
            execution_profile_id: String::new(),
            enabled: true,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeDefaults {
    pub execution_profile_id: Option<ExecutionProfileId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeState {
    #[serde(default)]
    pub active_sessions: BTreeMap<RoomId, SessionRecord>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub room_id: RoomId,
    pub execution_profile_id: ExecutionProfileId,
    pub session_id: String,
}

fn default_state_version() -> StateVersion {
    STATE_VERSION
}

fn default_true() -> bool {
    true
}
