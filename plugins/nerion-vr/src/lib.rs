//! Nerion VR poses on Pumpkin (`nerion:vr`, format v1 in Nerion's `odd/tasks/nerion-vr.md`):
//! a player's head and hand pose goes to the players around who registered the channel, with the
//! same checks and distance thinning as NerionServerSDK's relay. Built as a Pumpkin Wasm plugin
//! (plugin API v0.1) for the Pumpkin Nerion ships on Android.
//!
//! [`proto`] is a copy of Nerion Nativo's `nerion-vr-proto`; `tests/shared_vectors.rs` checks it
//! against the same vectors as every other codec.
pub mod proto;

#[cfg(target_arch = "wasm32")]
mod plugin {
    use super::proto::{self, relay};
    use pumpkin_plugin_api::events::{
        EventHandler, EventPriority, PlayerCustomPayloadEvent, PlayerJoinEvent, PlayerLeaveEvent,
        PlayerRegisterChannelEvent, PlayerUnregisterChannelEvent,
    };
    use pumpkin_plugin_api::{Context, Plugin, PluginMetadata, Server, events_wit, register_plugin};
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;
    use std::time::Instant;

    #[derive(Default)]
    struct State {
        /// Players (UUID text) whose clients registered `nerion:vr`.
        listening: HashSet<String>,
        limits: HashMap<String, relay::RateLimit>,
        /// Players who sent a pose in VR, to tell viewers when they leave.
        in_vr: HashSet<String>,
    }

    static STATE: Mutex<Option<State>> = Mutex::new(None);

    fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
        let mut s = STATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        f(s.get_or_insert_with(State::default))
    }

    fn key(player: &pumpkin_plugin_api::player::Player) -> String {
        format!("{:?}", player.get_id())
    }

    struct Payloads;
    impl EventHandler<PlayerCustomPayloadEvent> for Payloads {
        fn handle(
            &self,
            server: Server,
            event: events_wit::PlayerCustomPayloadEventData,
        ) -> events_wit::PlayerCustomPayloadEventData {
            if event.channel == proto::CHANNEL {
                relay_pose(&server, &event.player, &event.data);
            }
            event
        }
    }

    fn relay_pose(server: &Server, sender: &pumpkin_plugin_api::player::Player, data: &[u8]) {
        let id = key(sender);
        let allowed = with(|s| s.limits.entry(id.clone()).or_default().try_take(Instant::now()));
        if !allowed {
            return;
        }
        let Ok(pose) = proto::decode(data) else { return };
        if relay::reject(&pose).is_some() {
            return;
        }
        with(|s| {
            if pose.vr_active {
                s.in_vr.insert(id.clone());
            } else {
                s.in_vr.remove(&id);
            }
        });
        let entity_id = sender.as_entity().get_id() as i32;
        let out = proto::forwarded(entity_id, data);
        let (x, y, z) = sender.get_position();
        for viewer in server.get_players_in_world(sender.get_world()) {
            let vid = key(&viewer);
            if vid == id || !with(|s| s.listening.contains(&vid)) {
                continue;
            }
            let (vx, vy, vz) = viewer.get_position();
            let d = ((vx - x).powi(2) + (vy - y).powi(2) + (vz - z).powi(2)).sqrt();
            if relay::forward(pose.sequence, d, pose.vr_active) {
                send(&viewer, &out);
            }
        }
    }

    /// A `nerion:vr` payload to a Java player (Bedrock players have no such channel).
    fn send(viewer: &pumpkin_plugin_api::player::Player, data: &[u8]) {
        if let Some(java) = viewer.as_java() {
            java.send_custom_payload(proto::CHANNEL, data);
        }
    }

    /// Announces `nerion:vr` (`minecraft:register`) to each player who joins: Fabric clients only
    /// send on channels the server announced.
    struct Joined;
    impl EventHandler<PlayerJoinEvent> for Joined {
        fn handle(
            &self,
            _server: Server,
            event: events_wit::PlayerJoinEventData,
        ) -> events_wit::PlayerJoinEventData {
            if let Some(java) = event.player.as_java() {
                java.send_custom_payload("minecraft:register", proto::CHANNEL.as_bytes());
            }
            event
        }
    }

    struct Registered;
    impl EventHandler<PlayerRegisterChannelEvent> for Registered {
        fn handle(
            &self,
            _server: Server,
            event: events_wit::PlayerRegisterChannelEventData,
        ) -> events_wit::PlayerRegisterChannelEventData {
            if event.channel == proto::CHANNEL {
                let id = key(&event.player);
                with(|s| s.listening.insert(id));
            }
            event
        }
    }

    struct Unregistered;
    impl EventHandler<PlayerUnregisterChannelEvent> for Unregistered {
        fn handle(
            &self,
            _server: Server,
            event: events_wit::PlayerUnregisterChannelEventData,
        ) -> events_wit::PlayerUnregisterChannelEventData {
            if event.channel == proto::CHANNEL {
                let id = key(&event.player);
                with(|s| s.listening.remove(&id));
            }
            event
        }
    }

    struct Left;
    impl EventHandler<PlayerLeaveEvent> for Left {
        fn handle(
            &self,
            server: Server,
            event: events_wit::PlayerLeaveEventData,
        ) -> events_wit::PlayerLeaveEventData {
            let id = key(&event.player);
            let was_in_vr = with(|s| {
                s.listening.remove(&id);
                s.limits.remove(&id);
                s.in_vr.remove(&id)
            });
            if was_in_vr {
                let off = proto::forwarded(
                    event.player.as_entity().get_id() as i32,
                    &proto::encode(&proto::Pose::not_in_vr(0)),
                );
                for viewer in server.get_players_in_world(event.player.get_world()) {
                    let vid = key(&viewer);
                    if vid != id && with(|s| s.listening.contains(&vid)) {
                        send(&viewer, &off);
                    }
                }
            }
            event
        }
    }

    struct NerionVr;
    impl Plugin for NerionVr {
        fn new() -> Self {
            NerionVr
        }

        fn metadata(&self) -> PluginMetadata {
            PluginMetadata {
                name: "nerion-vr".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                authors: vec!["feeldev".into()],
                description: "Nerion VR poses (nerion:vr): VR players' head and hands to the players around".into(),
                dependencies: vec![],
                permissions: vec![],
            }
        }

        fn on_load(&self, context: Context) -> Result<(), String> {
            context.register_event_handler::<PlayerCustomPayloadEvent, _>(Payloads, EventPriority::Normal, false)?;
            context.register_event_handler::<PlayerJoinEvent, _>(Joined, EventPriority::Lowest, false)?;
            context.register_event_handler::<PlayerRegisterChannelEvent, _>(Registered, EventPriority::Normal, false)?;
            context.register_event_handler::<PlayerUnregisterChannelEvent, _>(Unregistered, EventPriority::Normal, false)?;
            context.register_event_handler::<PlayerLeaveEvent, _>(Left, EventPriority::Lowest, false)?;
            Ok(())
        }
    }

    register_plugin!(NerionVr);
}
