use std::time::Instant;

use serde_json::Value;

use super::inbox::{self, Limit, Message};
use crate::relic;

pub(crate) type Sender = inbox::Sender<Event>;
pub(crate) type Receiver = inbox::Receiver<Event>;

pub(crate) fn channel() -> (Sender, Receiver) {
    inbox::channel(
        "presentation",
        Limit {
            items: 64,
            bytes: 128 * 1024,
        },
        Limit {
            items: 32,
            bytes: 8 * 1024 * 1024,
        },
    )
}

#[derive(Debug)]
pub(crate) enum Event {
    Connected(String),
    Disconnected(String),
    Player(PlayerStatus),
    AssetRefreshed(relic::AssetRefresh),
    OverlayVisible(bool),
    HudVisible(bool),
    DucatScene {
        session: super::session::Session,
        scene: Option<crate::ducats::Scene>,
        expires: Instant,
    },
    RelicScene {
        context: relic::Context,
        scene: relic::Scene,
        deadline: Option<Instant>,
    },
    RelicStart(relic::Context),
    RelicDismiss,
    InteractionToggle,
}

#[derive(Debug, Default)]
pub(crate) struct PlayerStatus {
    pub(crate) phase: Option<String>,
    pub(crate) pid: Option<u32>,
    pub(crate) debug_output_active: Option<bool>,
    pub(crate) debug_lines: u64,
}

impl PlayerStatus {
    pub(crate) fn from_snapshot(snapshot: &Value) -> Self {
        Self {
            phase: snapshot
                .pointer("/data/game/phase")
                .and_then(Value::as_str)
                .map(str::to_owned),
            pid: snapshot
                .pointer("/data/game/pid")
                .and_then(Value::as_u64)
                .and_then(|pid| u32::try_from(pid).ok()),
            debug_output_active: snapshot
                .pointer("/data/collector/debug_output_active")
                .and_then(Value::as_bool),
            debug_lines: snapshot
                .pointer("/data/collector/debug_output_lines_observed")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        }
    }
}

impl Message for Event {
    fn bytes(&self) -> usize {
        size_of::<Self>()
            + match self {
                Self::Connected(text) | Self::Disconnected(text) => text.capacity(),
                Self::Player(player) => player.phase.as_ref().map_or(0, String::capacity),
                Self::AssetRefreshed(asset) => asset.bytes(),
                Self::RelicScene { scene, .. } => scene.bytes(),
                Self::DucatScene {
                    scene: Some(scene), ..
                } => {
                    scene.hover.title.capacity()
                        + scene.parent.as_ref().map_or(0, |p| p.name.capacity())
                }
                _ => 0,
            }
    }

    fn control(&self) -> bool {
        matches!(
            self,
            Self::RelicScene { .. }
                | Self::RelicStart(_)
                | Self::RelicDismiss
                | Self::InteractionToggle
                | Self::OverlayVisible(_)
                | Self::HudVisible(_)
        )
    }

    fn replaces(&self, queued: &Self) -> bool {
        match (self, queued) {
            (
                Self::Connected(_) | Self::Disconnected(_),
                Self::Connected(_) | Self::Disconnected(_),
            )
            | (Self::Player(_), Self::Player(_)) => true,
            (Self::DucatScene { .. }, Self::DucatScene { .. }) => true,
            (Self::AssetRefreshed(new), Self::AssetRefreshed(old)) => {
                new.source == old.source && new.image_name == old.image_name
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_projection_retains_only_hud_state() {
        let status = PlayerStatus::from_snapshot(&serde_json::json!({
            "data": {
                "game": {"phase": "game", "pid": 123},
                "collector": {"debug_output_active": true, "debug_output_lines_observed": 456},
                "inventory": {"raw": "x".repeat(1024 * 1024)}
            }
        }));
        assert_eq!(status.pid, Some(123));
        assert_eq!(status.phase.as_deref(), Some("game"));
        assert_eq!(status.debug_output_active, Some(true));
        assert_eq!(status.debug_lines, 456);
        assert!(Event::Player(status).bytes() < 1024);
    }

    #[test]
    fn slow_presentation_coalesces_status_but_keeps_scenes_lifecycle_and_toggles() {
        let (sender, receiver) = channel();
        let context = relic::Context::for_test(None);
        sender.send(Event::RelicStart(context.clone())).unwrap();
        for _ in 0..1000 {
            sender.send(Event::Player(PlayerStatus::default())).unwrap();
        }
        sender
            .send(Event::RelicScene {
                context: context.clone(),
                scene: relic::Scene::Reading,
                deadline: None,
            })
            .unwrap();
        sender.send(Event::InteractionToggle).unwrap();
        sender
            .send(Event::RelicScene {
                context,
                scene: relic::Scene::Rewards(relic::Rewards::default()),
                deadline: None,
            })
            .unwrap();
        sender.send(Event::InteractionToggle).unwrap();
        sender.send(Event::RelicDismiss).unwrap();
        let events = receiver.drain(32);
        assert_eq!(events.len(), 7);
        assert!(matches!(events[0], Event::RelicStart(_)));
        assert!(matches!(events[1], Event::Player(_)));
        assert!(matches!(
            events[2],
            Event::RelicScene {
                scene: relic::Scene::Reading,
                ..
            }
        ));
        assert!(matches!(events[3], Event::InteractionToggle));
        assert!(matches!(
            events[4],
            Event::RelicScene {
                scene: relic::Scene::Rewards(_),
                ..
            }
        ));
        assert!(matches!(events[5], Event::InteractionToggle));
        assert!(matches!(events[6], Event::RelicDismiss));
    }

    #[test]
    fn asset_flood_cannot_take_lifecycle_capacity() {
        let (sender, receiver) = channel();
        for index in 0..64 {
            sender
                .send(Event::AssetRefreshed(relic::AssetRefresh {
                    source: "market".into(),
                    image_name: index.to_string(),
                    path: String::new(),
                    digest: String::new(),
                }))
                .unwrap();
        }
        let context = relic::Context::for_test(None);
        sender.send(Event::RelicStart(context.clone())).unwrap();
        sender
            .send(Event::RelicScene {
                context,
                scene: relic::Scene::Reading,
                deadline: None,
            })
            .unwrap();
        sender.send(Event::InteractionToggle).unwrap();
        assert_eq!(receiver.drain(128).len(), 67);
    }
}
