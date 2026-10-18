use std::collections::{BTreeMap, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use retinue_warframe::game_observer::ui::{DucatHover, DucatReader};
use serde::Deserialize;
use serde_json::Value;

use crate::daemon::{self, OutboundSender, PendingRequest};
use crate::runtime::{Worker, presentation, session::Session};

const ACTIVE_INTERVAL: Duration = Duration::from_millis(100);
const IDLE_INTERVAL: Duration = Duration::from_secs(1);
const HOVER_DELAY: Duration = Duration::from_millis(50);
const QUOTE_TTL: Duration = Duration::from_secs(60);
const RETRY_DELAY: Duration = Duration::from_secs(10);
pub(crate) const LEASE: Duration = Duration::from_millis(350);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Scene {
    pub(crate) hover: DucatHover,
    pub(crate) parent: Option<Parent>,
    pub(crate) listed: bool,
    pub(crate) quote: Option<Quote>,
    pub(crate) failed: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Quote {
    pub(crate) lowest_sell: Option<u64>,
    pub(crate) quoted_at: u64,
    #[serde(default)]
    pub(crate) stale: bool,
}

impl Quote {
    fn fresh(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        !self.stale
            && now.as_millis().saturating_sub(self.quoted_at as u128) < QUOTE_TTL.as_millis()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Parent {
    pub(crate) name: String,
    pub(crate) owned: bool,
    pub(crate) mastered: bool,
    pub(crate) subsumed: bool,
}

#[derive(Deserialize)]
struct Item {
    name: String,
    ducats: u64,
    slug: Option<String>,
    parent: Option<Parent>,
    quote: Option<Quote>,
}

enum Request {
    Inventory,
    Quote(String),
}

struct Pending {
    request: PendingRequest,
    kind: Request,
}

#[derive(Default)]
struct Pricing {
    current: Option<Scene>,
    expires: Option<Instant>,
    changed: Option<Instant>,
    pending: Option<Pending>,
    items: Vec<Item>,
    warm: VecDeque<String>,
    failures: BTreeMap<String, Instant>,
    kiosk: bool,
    inventory_due: Option<Instant>,
    last_inventory: Option<Instant>,
}

impl Pricing {
    fn kiosk(&mut self, open: bool, now: Instant) {
        if open != self.kiosk {
            self.kiosk = open;
            self.warm.clear();
            self.inventory_due = open.then_some(now);
            self.failures.retain(|_, until| *until > now);
        }
    }

    fn observe(&mut self, hover: Option<DucatHover>, now: Instant) {
        let same = match (&self.current, &hover) {
            (Some(old), Some(new)) => {
                old.hover.title == new.title && old.hover.ducats == new.ducats
            }
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.changed = Some(now);
        }
        self.current = hover.map(|hover| Scene {
            hover,
            parent: None,
            listed: false,
            quote: None,
            failed: false,
        });
        self.expires = self.current.as_ref().map(|_| now + LEASE);
        self.update_scene();
        if self
            .current
            .as_ref()
            .is_some_and(|s| self.item(&s.hover).is_none())
            && self.inventory_due.is_none()
            && self
                .last_inventory
                .is_some_and(|last| now.duration_since(last) >= RETRY_DELAY)
        {
            self.inventory_due = Some(now);
        }
    }

    fn read_failed(&mut self, now: Instant) {
        if self.expires.is_none_or(|expires| now >= expires) {
            self.observe(None, now);
        }
    }

    fn item(&self, hover: &DucatHover) -> Option<&Item> {
        let mut matches = self.items.iter().filter(|item| {
            item.name.eq_ignore_ascii_case(&hover.title) && item.ducats == hover.ducats
        });
        let item = matches.next()?;
        matches.next().is_none().then_some(item)
    }

    fn update_scene(&mut self) {
        let Some(scene) = self.current.as_ref() else {
            return;
        };
        let item = self.item(&scene.hover);
        let parent = item.and_then(|item| item.parent.clone()).filter(|parent| {
            !scene.hover.title.eq_ignore_ascii_case(&parent.name)
                && !scene
                    .hover
                    .title
                    .eq_ignore_ascii_case(&format!("{} Blueprint", parent.name))
        });
        let listed = item.is_some_and(|item| item.slug.is_some());
        let quote = item.and_then(|item| item.quote.clone());
        let failed = item
            .and_then(|item| item.slug.as_ref())
            .is_some_and(|slug| self.failures.contains_key(slug));
        let scene = self.current.as_mut().unwrap();
        scene.parent = parent;
        scene.listed = listed;
        scene.quote = quote;
        scene.failed = failed;
    }

    fn needs_quote(&self, slug: &str, now: Instant) -> bool {
        !self.failures.get(slug).is_some_and(|until| *until > now)
            && self.items.iter().any(|item| {
                item.slug.as_deref() == Some(slug) && item.quote.as_ref().is_none_or(|q| !q.fresh())
            })
    }

    fn tick(
        &mut self,
        outbound: &OutboundSender,
        now: Instant,
    ) -> Option<(&'static str, Result<(), String>)> {
        if let Some(pending) = &self.pending {
            let reply = pending.request.poll()?;
            let pending = self.pending.take().unwrap();
            let outcome = match pending.kind {
                Request::Inventory => {
                    self.last_inventory = Some(now);
                    let result = reply.and_then(|reply| {
                        let items = reply
                            .pointer("/data/items")
                            .ok_or("missing ducat inventory")?;
                        let mut items: Vec<Item> = serde_json::from_value(items.clone())
                            .map_err(|e| format!("invalid ducat inventory: {e}"))?;
                        for item in &mut items {
                            if let Some(slug) = &item.slug
                                && let Some(cached) = self
                                    .items
                                    .iter()
                                    .filter(|old| old.slug.as_ref() == Some(slug))
                                    .filter_map(|old| old.quote.as_ref())
                                    .max_by_key(|quote| quote.quoted_at)
                                && item
                                    .quote
                                    .as_ref()
                                    .is_none_or(|q| q.quoted_at < cached.quoted_at)
                            {
                                item.quote = Some(cached.clone());
                            }
                        }
                        self.items = items;
                        self.warm = if self.kiosk {
                            self.items
                                .iter()
                                .filter(|item| item.quote.as_ref().is_none_or(|q| !q.fresh()))
                                .filter_map(|item| item.slug.clone())
                                .collect()
                        } else {
                            VecDeque::new()
                        };
                        if let Some(scene) = &self.current
                            && self.item(&scene.hover).is_none()
                        {
                            return Err(format!(
                                "no unique owned item for {} ({} ducats)",
                                scene.hover.title, scene.hover.ducats
                            ));
                        }
                        Ok(())
                    });
                    if result.is_err() && self.kiosk {
                        self.inventory_due = Some(now + RETRY_DELAY);
                    }
                    ("ducat_inventory", result)
                }
                Request::Quote(slug) => {
                    let result = reply.and_then(|reply| quote(&reply, &slug)).map(|quote| {
                        for item in &mut self.items {
                            if item.slug.as_ref() == Some(&slug) {
                                item.quote = Some(quote.clone());
                            }
                        }
                        self.failures.remove(&slug);
                    });
                    if result.is_err() {
                        self.failures.insert(slug.clone(), now + RETRY_DELAY);
                    }
                    ("ducat_market", result.map_err(|e| format!("{slug}: {e}")))
                }
            };
            self.update_scene();
            return Some(outcome);
        }
        if !self.kiosk {
            return None;
        }
        if self.inventory_due.is_some_and(|due| now >= due) {
            self.inventory_due = None;
            match daemon::begin_ducat_inventory(outbound) {
                Ok(request) => {
                    self.pending = Some(Pending {
                        request,
                        kind: Request::Inventory,
                    })
                }
                Err(error) => {
                    self.inventory_due = Some(now + RETRY_DELAY);
                    return Some(("ducat_inventory", Err(error)));
                }
            }
            return None;
        }
        let foreground = self
            .current
            .as_ref()
            .and_then(|s| self.item(&s.hover))
            .and_then(|item| item.slug.as_ref())
            .filter(|slug| self.needs_quote(slug, now))
            .filter(|_| {
                self.changed
                    .is_some_and(|time| now.duration_since(time) >= HOVER_DELAY)
            })
            .cloned();
        let slug = foreground.or_else(|| {
            while let Some(slug) = self.warm.pop_front() {
                if self.needs_quote(&slug, now) {
                    return Some(slug);
                }
            }
            None
        })?;
        match daemon::begin_market_quote(outbound, slug.clone()) {
            Ok(request) => {
                self.pending = Some(Pending {
                    request,
                    kind: Request::Quote(slug),
                })
            }
            Err(error) => {
                self.failures.insert(slug.clone(), now + RETRY_DELAY);
                self.update_scene();
                return Some(("ducat_market", Err(format!("{slug}: {error}"))));
            }
        }
        None
    }
}

fn quote(reply: &Value, slug: &str) -> Result<Quote, String> {
    let rows = reply
        .pointer("/data/quotes")
        .and_then(Value::as_array)
        .ok_or("missing tooltip quote")?;
    let [item] = rows.as_slice() else {
        return Err("ambiguous tooltip quote".into());
    };
    if item["slug"].as_str() != Some(slug) {
        return Err("tooltip quote identity changed".into());
    }
    if let Some(error) = item.get("error").filter(|error| !error.is_null()) {
        return Err(format!("tooltip quote unavailable: {error}"));
    }
    let quote: Quote = serde_json::from_value(item["quote"].clone())
        .map_err(|e| format!("invalid tooltip quote: {e}"))?;
    if !quote.fresh() {
        return Err("tooltip quote is stale".into());
    }
    Ok(quote)
}

pub(crate) fn spawn(
    session: Session,
    outbound: OutboundSender,
    ui: presentation::Sender,
    stopping: Arc<AtomicBool>,
) -> Result<Worker, String> {
    let handle = std::thread::Builder::new()
        .name("wfcompanion-ducats".into())
        .spawn(move || {
            let opened = session.read(DucatReader::open_for_identity);
            session.record("ducat_bindings", &opened);
            let Ok(mut reader) = opened else {
                return;
            };
            let mut pricing = Pricing::default();
            let mut next = Instant::now();
            let mut failures = 0;
            while !stopping.load(Ordering::Relaxed) && session.is_current() {
                let now = Instant::now();
                let mut publish = false;
                if now >= next {
                    let result = session.check().and_then(|_| reader.read());
                    let interval = match result {
                        Ok(state) => {
                            failures = 0;
                            pricing.kiosk(state.kiosk_open, now);
                            if state.kiosk_open {
                                session.record("ducat_kiosk", &Ok::<_, String>(()));
                                if state.hover.is_some() {
                                    session.record("ducat_tooltip", &Ok::<_, String>(()));
                                }
                            }
                            pricing.observe(state.hover, now);
                            if state.kiosk_open {
                                ACTIVE_INTERVAL
                            } else {
                                IDLE_INTERVAL
                            }
                        }
                        Err(error) => {
                            pricing.read_failed(now);
                            failures += 1;
                            if failures >= 3 {
                                pricing.kiosk(false, now);
                                session.record("ducat_tooltip", &Err::<(), _>(error));
                            }
                            if failures < 3 {
                                ACTIVE_INTERVAL
                            } else {
                                IDLE_INTERVAL
                            }
                        }
                    };
                    next = Instant::now() + interval;
                    publish = true;
                }
                if let Some((check, result)) = pricing.tick(&outbound, now) {
                    session.record(check, &result);
                    publish = true;
                }
                if publish {
                    let _ = ui.send(presentation::Event::DucatScene {
                        session: session.clone(),
                        scene: pricing.current.clone(),
                        expires: pricing.expires.unwrap_or(now),
                    });
                }
                std::thread::sleep(
                    Duration::from_millis(25).min(next.saturating_duration_since(Instant::now())),
                );
            }
            let _ = ui.send(presentation::Event::DucatScene {
                session,
                scene: None,
                expires: Instant::now(),
            });
        })
        .map_err(|e| format!("could not start ducat observer: {e}"))?;
    Ok(Worker::new("ducats", handle))
}

#[cfg(test)]
mod tests;
