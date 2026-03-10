use crate::state::{
    CrewGlueState, ExecutionProfile, ProviderProfile, RoomBinding, RoomId, RoomInventory,
    SpaceBinding, Workspace,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResolveError {
    #[error("room '{0}' not found in inventory")]
    UnknownRoom(RoomId),
    #[error("no binding or default execution profile is configured for room '{0}'")]
    NoExecutionProfile(RoomId),
    #[error("execution profile '{0}' was referenced but not found")]
    UnknownExecutionProfile(String),
    #[error("provider '{0}' was referenced but not found")]
    UnknownProvider(String),
    #[error("workspace '{0}' was referenced but not found")]
    UnknownWorkspace(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingSource {
    Room(RoomBinding),
    Space(SpaceBinding),
    Default,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBinding {
    pub room: RoomInventory,
    pub source: BindingSource,
    pub execution_profile: ExecutionProfile,
    pub provider: ProviderProfile,
    pub workspace: Option<Workspace>,
}

pub fn resolve_room_binding(
    state: &CrewGlueState,
    room_id: &str,
) -> Result<ResolvedBinding, ResolveError> {
    let room = state
        .inventory
        .rooms
        .get(room_id)
        .cloned()
        .ok_or_else(|| ResolveError::UnknownRoom(room_id.to_string()))?;

    let (profile_id, source) = if let Some(binding) = state.room_bindings.get(room_id) {
        if binding.enabled {
            (
                binding.execution_profile_id.clone(),
                BindingSource::Room(binding.clone()),
            )
        } else {
            resolve_from_spaces_or_default(state, &room)?
        }
    } else {
        resolve_from_spaces_or_default(state, &room)?
    };

    let execution_profile = state
        .execution_profiles
        .get(&profile_id)
        .cloned()
        .ok_or_else(|| ResolveError::UnknownExecutionProfile(profile_id.clone()))?;
    let provider = state
        .providers
        .get(&execution_profile.provider_id)
        .cloned()
        .ok_or_else(|| ResolveError::UnknownProvider(execution_profile.provider_id.clone()))?;
    let workspace = match &execution_profile.workspace_id {
        Some(workspace_id) => Some(
            state
                .workspaces
                .get(workspace_id)
                .cloned()
                .ok_or_else(|| ResolveError::UnknownWorkspace(workspace_id.clone()))?,
        ),
        None => None,
    };

    Ok(ResolvedBinding {
        room,
        source,
        execution_profile,
        provider,
        workspace,
    })
}

fn resolve_from_spaces_or_default(
    state: &CrewGlueState,
    room: &RoomInventory,
) -> Result<(String, BindingSource), ResolveError> {
    for space_id in &room.space_ids {
        if let Some(binding) = state.space_bindings.get(space_id) {
            if binding.enabled && binding.apply_to_children {
                return Ok((
                    binding.execution_profile_id.clone(),
                    BindingSource::Space(binding.clone()),
                ));
            }
        }
    }

    state
        .defaults
        .execution_profile_id
        .clone()
        .map(|profile_id| (profile_id, BindingSource::Default))
        .ok_or_else(|| ResolveError::NoExecutionProfile(room.room_id.clone()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{BindingSource, resolve_room_binding};
    use crate::state::{
        BridgeDefaults, CrewGlueState, ExecutionProfile, InventorySnapshot, ProviderProfile,
        RoomBinding, RoomInventory, SpaceBinding, Workspace,
    };

    #[test]
    fn room_binding_wins_over_space_binding() {
        let mut state = CrewGlueState::default();
        state.providers.insert(
            "p1".into(),
            ProviderProfile {
                id: "p1".into(),
                provider: "openai".into(),
                ..Default::default()
            },
        );
        state.providers.insert(
            "p2".into(),
            ProviderProfile {
                id: "p2".into(),
                provider: "anthropic".into(),
                ..Default::default()
            },
        );
        state.execution_profiles.insert(
            "room".into(),
            ExecutionProfile {
                id: "room".into(),
                name: "Room".into(),
                provider_id: "p1".into(),
                ..Default::default()
            },
        );
        state.execution_profiles.insert(
            "space".into(),
            ExecutionProfile {
                id: "space".into(),
                name: "Space".into(),
                provider_id: "p2".into(),
                ..Default::default()
            },
        );
        state.inventory = InventorySnapshot {
            rooms: BTreeMap::from([(
                "!room:example.org".into(),
                RoomInventory {
                    room_id: "!room:example.org".into(),
                    space_ids: vec!["!space:example.org".into()],
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        state.space_bindings.insert(
            "!space:example.org".into(),
            SpaceBinding {
                space_id: "!space:example.org".into(),
                execution_profile_id: "space".into(),
                ..Default::default()
            },
        );
        state.room_bindings.insert(
            "!room:example.org".into(),
            RoomBinding {
                room_id: "!room:example.org".into(),
                execution_profile_id: "room".into(),
                ..Default::default()
            },
        );

        let resolved = resolve_room_binding(&state, "!room:example.org").unwrap();
        assert!(matches!(resolved.source, BindingSource::Room(_)));
        assert_eq!(resolved.execution_profile.id, "room");
    }

    #[test]
    fn resolves_default_profile() {
        let mut state = CrewGlueState::default();
        state.providers.insert(
            "p1".into(),
            ProviderProfile {
                id: "p1".into(),
                provider: "openai".into(),
                ..Default::default()
            },
        );
        state.workspaces.insert(
            "ws".into(),
            Workspace {
                id: "ws".into(),
                name: "Repo".into(),
                root_dir: "/tmp/repo".into(),
                ..Default::default()
            },
        );
        state.execution_profiles.insert(
            "default".into(),
            ExecutionProfile {
                id: "default".into(),
                name: "Default".into(),
                provider_id: "p1".into(),
                workspace_id: Some("ws".into()),
                ..Default::default()
            },
        );
        state.defaults = BridgeDefaults {
            execution_profile_id: Some("default".into()),
        };
        state.inventory.rooms.insert(
            "!room:example.org".into(),
            RoomInventory {
                room_id: "!room:example.org".into(),
                ..Default::default()
            },
        );

        let resolved = resolve_room_binding(&state, "!room:example.org").unwrap();
        assert!(matches!(resolved.source, BindingSource::Default));
        assert_eq!(resolved.workspace.unwrap().id, "ws");
    }
}
