use super::{Payload, Payloads};
use crate::utils::{Keyed, NamespacedKey, UniquelyIdentified};
use bevy::asset::io::AssetSourceId;
use bevy::asset::{AssetPath, LoadedFolder};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use uuid::{Uuid, uuid};

#[derive(Default, Resource)]
struct PayloadHold {
    loading_scopes: HashSet<Handle<LoadedFolder>>,
    loaded_scopes: HashSet<Handle<LoadedFolder>>,
}

#[derive(Event)]
pub struct PayloadFetchingComplete;

fn monitor_folder_loads(
    mut folder_events_reader: MessageReader<AssetEvent<LoadedFolder>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut payload_hold: ResMut<PayloadHold>,
) {
    let mut completed = false;
    for folder_event in folder_events_reader.read() {
        let AssetEvent::LoadedWithDependencies { id } = folder_event else {
            continue;
        };
        let handle = asset_server.get_id_handle(*id).unwrap();
        if payload_hold.loading_scopes.remove(&handle) {
            payload_hold.loaded_scopes.insert(handle);
            completed = true;
        }
    }
    // bevy never emits `LoadedWithDependencies` when a folder load fails (e.g.
    // a dimension with no payload directory) or when a folder is (re)loaded
    // while already loaded. Poll for those terminal states so a fetch never
    // waits forever; a missing folder counts as an empty scope.
    let mut settled: Vec<(Handle<LoadedFolder>, bool)> = Vec::new();
    for handle in payload_hold.loading_scopes.iter() {
        let Some((state, _, recursive)) = asset_server.get_load_states(handle.id()) else {
            continue;
        };
        if state.is_failed() {
            settled.push((handle.clone(), true));
        } else if state.is_loaded() && (recursive.is_loaded() || recursive.is_failed()) {
            settled.push((handle.clone(), false));
        }
    }
    for (handle, failed) in settled {
        if failed {
            warn!(
                "Payload scope folder failed to load; treating it as empty: {:?}",
                asset_server.get_path(handle.id())
            );
        }
        if payload_hold.loading_scopes.remove(&handle) {
            if !failed {
                payload_hold.loaded_scopes.insert(handle);
            }
            completed = true;
        }
    }
    if completed && payload_hold.loading_scopes.is_empty() {
        commands.trigger(PayloadFetchingComplete);
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum PayloadScopeId {
    #[default]
    Global,
    Dimension(NamespacedKey),
}

impl PayloadScopeId {
    fn build<'src_id>(&self, source: impl Into<AssetSourceId<'src_id>>) -> PayloadScope {
        match self {
            Self::Global => PayloadScope::new(AssetPath::from("global").with_source(source)),
            Self::Dimension(key) => PayloadScope::new(
                AssetPath::from("dim")
                    .resolve(&key.into())
                    .with_source(source),
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PayloadScope {
    root: AssetPath<'static>,
}

impl PayloadScope {
    pub fn new<'path>(root: impl Into<AssetPath<'path>>) -> Self {
        Self {
            root: root.into().into_owned(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct PayloadSourceId {
    asset_source_id: AssetSourceId<'static>,
    uuid: Uuid,
}

impl UniquelyIdentified for PayloadSourceId {
    fn unique_id(&self) -> Uuid {
        self.uuid
    }
}

impl PayloadSourceId {
    pub fn new<'src_id>(
        asset_source_id: impl Into<AssetSourceId<'src_id>>,
        uuid: &impl UniquelyIdentified,
    ) -> Self {
        Self {
            asset_source_id: asset_source_id.into().into_owned(),
            uuid: uuid.unique_id().clone(),
        }
    }
    pub(crate) fn new_embers() -> Self {
        Self {
            asset_source_id: AssetSourceId::Default,
            uuid: EMBERS_PAYLOAD_SOURCE_UUID.clone(),
        }
    }
}

struct PayloadSource {
    id: PayloadSourceId,
    scopes: Vec<PayloadScope>, // TODO optimize memory layout
}

#[derive(Resource)]
pub struct PayloadManager {
    scope_ids: Vec<PayloadScopeId>,
    sources: Vec<PayloadSource>,
}

impl PayloadManager {
    fn new() -> Self {
        Self {
            scope_ids: Vec::with_capacity(2),
            sources: Vec::with_capacity(1),
        }
    }
}

#[derive(Event)]
pub struct FetchPayloadScopeRequest(PayloadScopeId);

impl FetchPayloadScopeRequest {
    pub fn new(scope_id: PayloadScopeId) -> Self {
        Self(scope_id)
    }
}

fn handle_fetch_scope_request(
    request: On<FetchPayloadScopeRequest>,
    asset_server: Res<AssetServer>,
    mut payload_manager: ResMut<PayloadManager>,
    mut payload_hold: ResMut<PayloadHold>,
) {
    let FetchPayloadScopeRequest(scope_id) = &*request;
    payload_manager.scope_ids.push(scope_id.clone());
    for source in &mut payload_manager.sources {
        let scope = scope_id.build(source.id.asset_source_id.clone());
        payload_hold
            .loading_scopes
            .insert(asset_server.load_folder(&scope.root));
        source.scopes.push(scope);
    }
}

#[derive(Event)]
pub struct EvictPayloadScopeRequest(PayloadScopeId);

impl EvictPayloadScopeRequest {
    pub fn new(scope_id: PayloadScopeId) -> Self {
        Self(scope_id)
    }
}

fn handle_evict_scope_request(
    request: On<EvictPayloadScopeRequest>,
    asset_server: Res<AssetServer>,
    mut payload_manager: ResMut<PayloadManager>,
    mut payload_hold: ResMut<PayloadHold>,
) {
    let EvictPayloadScopeRequest(scope_id) = &*request;
    assert!(
        payload_manager
            .scope_ids
            .last()
            .is_some_and(|id| id == scope_id),
        "Only the topmost scope may be evicted."
    );
    payload_manager.scope_ids.pop();
    for source in &mut payload_manager.sources {
        payload_hold.loaded_scopes.remove(
            &asset_server
                .get_handle(&source.scopes.pop().unwrap().root)
                .unwrap(),
        );
    }
}

#[derive(Event)]
pub struct MountPayloadSourceRequest(PayloadSourceId);

impl MountPayloadSourceRequest {
    pub fn new(source_id: PayloadSourceId) -> Self {
        Self(source_id)
    }
}

fn handle_mount_source_request(
    request: On<MountPayloadSourceRequest>,
    asset_server: Res<AssetServer>,
    mut payload_manager: ResMut<PayloadManager>,
    mut payload_hold: ResMut<PayloadHold>,
) {
    let MountPayloadSourceRequest(source_id) = &*request;
    let PayloadManager { scope_ids, sources } = &mut *payload_manager;
    sources.push(PayloadSource {
        id: source_id.clone(),
        scopes: scope_ids
            .iter()
            .map(|scope_id| scope_id.build(source_id.asset_source_id.clone()))
            .inspect(|scope| {
                payload_hold
                    .loading_scopes
                    .insert(asset_server.load_folder(&scope.root));
            })
            .collect(),
    });
}

#[derive(Event)]
pub struct UnmountPayloadSourceRequest(PayloadSourceId);

impl UnmountPayloadSourceRequest {
    pub fn new(source_id: PayloadSourceId) -> Self {
        Self(source_id)
    }
}

fn handle_unmount_source_request(
    request: On<UnmountPayloadSourceRequest>,
    asset_server: Res<AssetServer>,
    mut payload_manager: ResMut<PayloadManager>,
    mut payload_hold: ResMut<PayloadHold>,
) {
    let UnmountPayloadSourceRequest(source_id) = &*request;
    match payload_manager
        .sources
        .iter()
        .position(|PayloadSource { id, .. }| source_id == id)
    {
        Some(index) => {
            for scope in payload_manager.sources.remove(index).scopes {
                payload_hold
                    .loaded_scopes
                    .remove(&asset_server.get_handle(scope.root).unwrap());
            }
        }
        None => error!(
            "The specified source could not be unloaded because it is not loaded: {:?}",
            source_id
        ),
    }
}

#[derive(Event)]
pub struct RefetchPayloadRequest;

fn handle_reload_request(
    request: On<RefetchPayloadRequest>,
    asset_server: Res<AssetServer>,
    payload_manager: Res<PayloadManager>,
    mut payload_hold: ResMut<PayloadHold>,
) {
    let RefetchPayloadRequest = &*request;
    let PayloadHold {
        loading_scopes,
        loaded_scopes,
    } = &mut *payload_hold;
    loading_scopes.extend(loaded_scopes.drain());
    for source in &payload_manager.sources {
        for scope in &source.scopes {
            asset_server.reload(scope.root.clone());
        }
    }
}

#[inline]
pub fn resolve_handle<'path, P: Payload>(
    payload_manager: &PayloadManager,
    asset_server: &AssetServer,
    assets: &Assets<P>,
    path: impl Into<AssetPath<'path>>,
) -> Option<Handle<P>> {
    resolve_source_handle(payload_manager, asset_server, assets, path)
        .unzip()
        .1
}

pub fn resolve_source_handle<'path, P: Payload>(
    payload_manager: &PayloadManager,
    asset_server: &AssetServer,
    assets: &Assets<P>,
    path: impl Into<AssetPath<'path>>,
) -> Option<(PayloadSourceId, Handle<P>)> {
    let path = P::payload_root().resolve(&path.into());
    let sourceless_path = path.clone().with_source(AssetSourceId::Default).to_string();
    for source in payload_manager.sources.iter().rev() {
        for scope in source.scopes.iter().rev() {
            if let Some(handle) = asset_server.get_handle(scope.root.resolve(&path)) {
                return Some((source.id.clone(), handle));
            }
        }
        let uuid = Uuid::new_v5(&source.id.uuid, sourceless_path.as_bytes());
        if assets.contains(uuid.clone()) {
            return Some((source.id.clone(), Handle::Uuid(uuid, PhantomData)));
        }
    }
    None
}

#[inline]
pub fn resolve_payload<'path, 'pld, P: Payload>(
    payload_manager: &PayloadManager,
    asset_server: &AssetServer,
    assets: &'pld Assets<P>,
    path: impl Into<AssetPath<'path>>,
) -> Option<&'pld P> {
    resolve_handle(payload_manager, asset_server, assets, path)
        .map(|handle| assets.get(&handle).unwrap())
}

#[inline]
pub fn resolve_source_payload<'path, 'pld, P: Payload>(
    payload_manager: &PayloadManager,
    asset_server: &AssetServer,
    assets: &'pld Assets<P>,
    path: impl Into<AssetPath<'path>>,
) -> Option<(PayloadSourceId, &'pld P)> {
    resolve_source_handle(payload_manager, asset_server, assets, path)
        .map(|(source, handle)| (source, assets.get(&handle).unwrap()))
}

#[derive(Default, Resource)]
pub struct InjectedPayloads {
    pub(super) source_uuids: HashMap<Uuid, Uuid>,
}

pub(crate) static EMBERS_PAYLOAD_SOURCE_UUID: Uuid = uuid!("9e037d1a-048d-4784-8ec1-0655421951b1");

pub fn scan_source_uuid<A: Asset>(
    injected_payloads: &InjectedPayloads,
    payload_manager: &PayloadManager,
    asset_server: &AssetServer,
    id: impl Into<AssetId<A>>,
) -> Option<Uuid> {
    let id = id.into();
    match id {
        AssetId::Index { .. } => asset_server
            .get_id_handle(id)
            .and_then(|handle| handle.path().cloned())
            .and_then(|path| {
                for source in payload_manager.sources.iter().rev() {
                    if *path.source() != source.id.asset_source_id {
                        continue;
                    }
                    for scope in source.scopes.iter().rev() {
                        if path.path().starts_with(scope.root.path()) {
                            return Some(source.id.uuid);
                        }
                    }
                }
                None
            }),
        AssetId::Uuid { uuid } => injected_payloads.source_uuids.get(&uuid).copied(),
    }
}

/// `path_format` is NOT a real format string! Only `{}` is replaced.
pub fn inject_payload_batch<P: Payload>(
    path_format: &'static str,
    source_uuid: Uuid,
    payload: impl IntoIterator<Item = (impl Keyed, impl Into<P>)> + Send + Sync + 'static,
) -> impl System<In = (), Out = ()> {
    let mut payload = Some(payload);
    IntoSystem::into_system(
        move |mut injected_payloads: ResMut<InjectedPayloads>, mut assets: ResMut<Assets<P>>| {
            if let Some(payload) = payload.take() {
                for (key, asset) in payload.into_iter() {
                    assets.inject(
                        &mut injected_payloads,
                        source_uuid,
                        &AssetPath::from(path_format.replace("{}", &*key.key().path_string())),
                        asset.into(),
                    );
                }
            } else {
                error!("The injection system is called multiple times. Skipping.");
            }
        },
    )
}

/// See [`inject_payload_batch`]
#[inline]
pub fn inject_keyed_payload_batch<P: Payload + Keyed>(
    path_format: &'static str,
    source_uuid: Uuid,
    payload: impl IntoIterator<Item = impl Into<P>, IntoIter: Send + Sync + 'static>,
) -> impl System<In = (), Out = ()> {
    inject_payload_batch::<P>(
        path_format,
        source_uuid,
        payload
            .into_iter()
            .map(|asset| asset.into())
            .map(|asset| (asset.key().clone(), asset)),
    )
}

/// See [`inject_payload_batch`]
#[inline]
pub(crate) fn inject_embers_payload_batch<P: Payload>(
    path_format: &'static str,
    payload: impl IntoIterator<Item = (impl Keyed, impl Into<P>)> + Send + Sync + 'static,
) -> impl System<In = (), Out = ()> {
    inject_payload_batch(path_format, EMBERS_PAYLOAD_SOURCE_UUID.clone(), payload)
}

/// See [`inject_payload_batch`]
#[inline]
pub(crate) fn inject_keyed_embers_payload_batch<P: Payload + Keyed>(
    path_format: &'static str,
    payload: impl IntoIterator<Item = impl Into<P>, IntoIter: Send + Sync + 'static>,
) -> impl System<In = (), Out = ()> {
    inject_keyed_payload_batch(path_format, EMBERS_PAYLOAD_SOURCE_UUID.clone(), payload)
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<PayloadHold>()
        .insert_resource(PayloadManager::new())
        .add_systems(Update, monitor_folder_loads)
        .add_observer(handle_fetch_scope_request)
        .add_observer(handle_evict_scope_request)
        .add_observer(handle_mount_source_request)
        .add_observer(handle_unmount_source_request)
        .add_observer(handle_reload_request)
        .init_resource::<InjectedPayloads>();
}
