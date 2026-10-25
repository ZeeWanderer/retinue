mod fixtures;
mod notes;
mod topology;

use super::*;
use crate::settings::rgb;
use retinue_ui::transition::ScalarTransition;
use widgets::{ACCENT, BAD, GOOD, MUTED, TEXT};

pub struct Builds {
    pub equipment: Value,
    pub groups: Value,
    pub account: Value,
    pub scroll: Scroll,
    offset: u32,
    tab: String,
    search: String,
    class: String,
    definition: String,
    instance: String,
    config: usize,
    list_scroll: Scroll,
    list_offset: u32,
    collapsed: bool,
    collapse: ScalarTransition,
    items: Value,
    sources: Value,
    detail: Value,
    scope: String,
    ordering: String,
    query: String,
    source_offset: usize,
    group: Value,
    member: String,
    planned: bool,
    dialog: Option<String>,
    name: String,
    target: String,
    notice: Option<String>,
    list_viewport: Option<Rect>,
    narrow: bool,
    preview: topology::Preview,
    added_group: String,
    notes: notes::Notes,
}
impl Default for Builds {
    fn default() -> Self {
        Self {
            equipment: Value::Null,
            groups: Value::Null,
            account: Value::Null,
            scroll: Default::default(),
            offset: 0,
            tab: "groups".into(),
            search: String::new(),
            class: "all".into(),
            definition: String::new(),
            instance: String::new(),
            config: 0,
            list_scroll: Default::default(),
            list_offset: 0,
            collapsed: false,
            collapse: Default::default(),
            items: Value::Null,
            sources: Value::Null,
            detail: Value::Null,
            scope: "public".into(),
            ordering: "score".into(),
            query: String::new(),
            source_offset: 0,
            group: Value::Null,
            member: String::new(),
            planned: false,
            dialog: None,
            name: String::new(),
            target: String::new(),
            notice: None,
            list_viewport: None,
            narrow: false,
            preview: Default::default(),
            added_group: String::new(),
            notes: Default::default(),
        }
    }
}
impl Builds {
    pub fn sample_preview(&mut self) {
        self.preview.tick(Instant::now() + Duration::from_secs(1));
    }
    pub fn dismiss(&mut self) {
        self.dialog = None;
    }
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.list_scroll
            .deadline(now)
            .into_iter()
            .chain(self.collapse.deadline(now, Duration::from_millis(16)))
            .chain(self.preview.deadline(now))
            .min()
    }
    pub fn wheel(&mut self, point: (f64, f64), delta: f32, smooth: bool) {
        self.preview.clear();
        if self
            .list_viewport
            .is_some_and(|r| r.contains(point.0, point.1))
        {
            self.list_scroll.by(delta, smooth, Instant::now());
        } else {
            self.scroll.by(delta, smooth, Instant::now());
        }
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        let next = self.scroll.offset(now).round() as u32;
        let list = self.list_scroll.offset(now).round() as u32;
        let changed = self.preview.tick(now)
            || next != self.offset
            || list != self.list_offset
            || self
                .collapse
                .deadline(now, Duration::from_millis(16))
                .is_some();
        self.offset = next;
        self.list_offset = list;
        changed
    }
    fn definition(&self) -> Value {
        entries(&self.equipment["definitions"])
            .iter()
            .find(|d| text(d, "id") == self.definition)
            .cloned()
            .unwrap_or(Value::Null)
    }
    fn instance(&self) -> Value {
        entries(&self.equipment["instances"])
            .iter()
            .find(|i| text(i, "instance_id") == self.instance)
            .cloned()
            .unwrap_or(Value::Null)
    }
    fn config(&self) -> Value {
        let instance = self.instance();
        entries(&instance["configs"])
            .iter()
            .find(|c| number(c, "config_index") as usize == self.config)
            .cloned()
            .unwrap_or(Value::Null)
    }
}

impl Pages {
    fn build_busy(&self) -> bool {
        self.pending.keys().any(|k| {
            k.starts_with("build:write") || matches!(k.as_str(), "build:plan" | "build:group")
        })
    }
    pub(super) fn build_event(&mut self, event: &Value) {
        let group = &event["data"];
        if text(group, "id") == text(&self.builds.group, "id") {
            self.builds.group = if event["action"] == "deleted" {
                Value::Null
            } else {
                group.clone()
            };
        }
        self.queue("groups", json!({"op":"build_group_list"}));
    }
    fn finish_build_scroll(&mut self, viewport: Rect, end: f32) {
        self.ui.end_scroll();
        self.builds.scroll.resize(
            self.ui.px(end + 20.0) as f32,
            viewport.height as f32,
            Instant::now(),
        );
        self.ui.scrollbar(
            "build:scroll",
            viewport,
            self.builds.scroll.maximum(),
            self.builds.offset as f32,
        );
    }
    pub(super) fn capture_build(&mut self, view: &str, select: &str) -> Result<(), String> {
        if !matches!(view, "equipment" | "discover" | "groups") {
            return Err(format!("Unknown Build Planner tab: {view}"));
        }
        self.builds.tab = view.into();
        if select.is_empty() {
            return Ok(());
        }
        match view {
            "equipment" => {
                let item = entries(&self.builds.equipment["definitions"])
                    .iter()
                    .find(|v| {
                        text(v, "id") == select || text(v, "name").eq_ignore_ascii_case(select)
                    })
                    .ok_or_else(|| format!("Equipment not found: {select}"))?;
                let id = text(item, "id").to_owned();
                self.build_signal(Signal {
                    key: format!("build:equipment:{id}"),
                    value: None,
                });
            }
            "discover" => {
                let id = select
                    .parse::<u64>()
                    .map_err(|_| "--select must be an Overframe build ID")?;
                self.queue(
                    "build:detail",
                    json!({"op":"build_detail","build_id":id,"refresh":false}),
                );
            }
            "groups" => {
                let group = entries(&self.builds.groups["groups"])
                    .iter()
                    .find(|v| {
                        text(v, "id") == select || text(v, "name").eq_ignore_ascii_case(select)
                    })
                    .ok_or_else(|| format!("Group not found: {select}"))?;
                let id = text(group, "id").to_owned();
                self.queue("build:group", json!({"op":"build_group_get","group_id":id}));
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    pub fn login_result(&mut self, result: Result<Value, String>) {
        self.login_active = false;
        match result {
            Ok(cookies) => self.queue(
                "build:write:login",
                json!({"op":"overframe_session_import","cookies":cookies}),
            ),
            Err(error) => self.error = Some(error),
        }
        self.dirty = true;
        self.layout();
    }
    pub(super) fn layout_builds(&mut self) {
        self.builds.preview.begin();
        self.builds.list_viewport = None;
        let w = self.ui.size.0 as f32 / self.ui.scale;
        let h = self.ui.size.1 as f32 / self.ui.scale;
        self.ui.bitmap(
            self.ui.rect(17.0, 17.0, 18.0, 18.0),
            include_bytes!("../../resources/assets/triangle-alert.png"),
        );
        self.ui.hit(
            "build:construction",
            self.ui.rect(10.0, 10.0, 30.0, 30.0),
            "Under construction",
            false,
        );
        let mut tab_x = 46.0;
        for (id, label) in [
            ("groups", "Groups"),
            ("equipment", "Equipment"),
            ("discover", "Discover"),
        ] {
            let width = self.ui.measure(label, 14.0, false) + 18.0;
            self.ui.chip(
                format!("build:tab:{id}"),
                self.ui.rect(tab_x, 10.0, width, 32.0),
                label,
                self.builds.tab == id,
            );
            tab_x += width + 6.0;
        }
        let signed = self.builds.account["authenticated"] == true;
        self.ui.button(
            "build:account",
            self.ui.rect(w - 146.0, 10.0, 132.0, 32.0),
            if self.login_active {
                "Cancel sign-in"
            } else if signed {
                "Sign out"
            } else {
                "Sign in"
            },
            false,
            true,
        );
        match self.builds.tab.as_str() {
            "discover" => self.build_discover(w, h),
            "groups" => self.build_groups(w, h),
            _ => self.build_equipment(w, h),
        }
        if let Some(error) = &self.error {
            self.ui.text(
                self.ui.rect(16.0, h - 28.0, w - 32.0, 24.0),
                error,
                13.0,
                BAD,
            );
        }
        if let Some(notice) = &self.builds.notice {
            self.ui
                .text(self.ui.rect(16.0, 48.0, w - 32.0, 24.0), notice, 13.0, GOOD);
        }
        self.builds
            .preview
            .render(&mut self.ui, self.builds.dialog.is_none());
        self.build_dialog(w, h);
    }
    fn build_equipment(&mut self, w: f32, h: f32) {
        if (w < 720.0) != self.builds.narrow {
            self.builds.narrow = w < 720.0;
            self.builds.collapsed = self.builds.narrow;
            self.builds.collapse.set(
                if self.builds.collapsed { 1.0 } else { 0.0 },
                Instant::now(),
                Duration::ZERO,
            );
        }
        let collapse = self.builds.collapse.value(Instant::now());
        let names = entries(&self.builds.equipment["definitions"]);
        let longest = names
            .iter()
            .map(|d| {
                retinue_ui::painter::text_width(
                    &self.ui.regular,
                    text(d, "name"),
                    13.0 * self.ui.scale,
                ) / self.ui.scale
                    + 60.0
            })
            .fold(195.0, f32::max)
            .min(295.0);
        let sidebar = 40.0 + (longest.min((w * 0.35).max(190.0)) - 40.0) * (1.0 - collapse);
        self.ui.tinted_bitmap(
            self.ui.rect(14.0, 77.0, 22.0, 22.0),
            include_bytes!("../../resources/ui/panel-left.png"),
            TEXT,
        );
        self.ui.hit(
            "build:collapse",
            self.ui.rect(8.0, 71.0, 34.0, 34.0),
            "Toggle equipment list",
            true,
        );
        if sidebar > 100.0 {
            self.ui.field(
                "build:search",
                self.ui.rect(45.0, 73.0, sidebar - 53.0, 32.0),
                &self.builds.search,
                "Equipment",
            );
            self.ui.choice(
                "build:class",
                self.ui.rect(12.0, 113.0, sidebar - 24.0, 29.0),
                &self.builds.class,
                &[
                    ("all", "All equipment"),
                    ("warframe", "Warframes"),
                    ("primary", "Primary"),
                    ("secondary", "Secondary"),
                    ("melee", "Melee"),
                    ("archwing", "Archwings"),
                    ("archgun", "Archguns"),
                    ("archmelee", "Archmelee"),
                    ("companion", "Companions"),
                    ("necramech", "Necramechs"),
                ],
            );
            let query = self.builds.search.to_lowercase();
            let instances = entries(&self.builds.equipment["instances"]);
            let definitions = entries(&self.builds.equipment["definitions"])
                .iter()
                .filter(|d| {
                    (query.is_empty() || text(d, "name").to_lowercase().contains(&query))
                        && (self.builds.class == "all" || text(d, "class") == self.builds.class)
                        && instances
                            .iter()
                            .any(|i| text(i, "definition_id") == text(d, "id"))
                })
                .cloned()
                .collect::<Vec<_>>();
            let viewport = self
                .ui
                .rect(7.0, 153.0, sidebar - 14.0, (h - 153.0).max(0.0));
            self.builds.list_viewport = Some(viewport);
            self.builds.list_scroll.resize(
                self.ui.px(definitions.len() as f32 * 47.0) as f32,
                viewport.height as f32,
                Instant::now(),
            );
            self.ui.begin_scroll(viewport, self.builds.list_offset);
            for (i, d) in definitions.iter().enumerate() {
                let top = i as f32 * 47.0;
                if self.ui.px(top + 47.0) < self.builds.list_offset
                    || self.ui.px(top) > self.builds.list_offset + viewport.height
                {
                    continue;
                }
                let r = self.ui.rect(0.0, top, sidebar - 14.0, 44.0);
                let id = text(d, "id");
                if self.builds.definition == id {
                    self.ui.fill(r, 6.0, ACCENT);
                }
                self.ui.image(
                    self.ui.rect(5.0, top + 3.0, 37.0, 37.0),
                    asset(&d["asset"]).as_ref(),
                );
                self.ui.text(
                    self.ui.rect(49.0, top, sidebar - 70.0, 44.0),
                    text(d, "name"),
                    13.0,
                    TEXT,
                );
                self.ui
                    .hit(format!("build:equipment:{id}"), r, text(d, "name"), true);
            }
            self.ui.end_scroll();
            self.ui.scrollbar(
                "build:list-scroll",
                viewport,
                self.builds.list_scroll.maximum(),
                self.builds.list_offset as f32,
            );
        }
        let x = sidebar + 10.0;
        let width = w - x - 12.0;
        let instance = self.builds.instance();
        let definition = self.builds.definition();
        if definition.is_null() {
            self.ui.text(
                self.ui.rect(x + 18.0, 100.0, width - 36.0, 40.0),
                if self.builds.equipment.is_null() {
                    "Loading equipment..."
                } else {
                    "Choose equipment"
                },
                18.0,
                MUTED,
            );
            return;
        }
        self.ui.bold(
            self.ui.rect(x, 72.0, width, 31.0),
            text(&definition, "name"),
            22.0,
            TEXT,
        );
        let copies = entries(&self.builds.equipment["instances"])
            .iter()
            .filter(|i| text(i, "definition_id") == self.builds.definition)
            .map(|i| {
                let id = text(i, "instance_id");
                let custom = text(i, "custom_name");
                (
                    id.to_owned(),
                    format!(
                        "{} ({})",
                        if custom.is_empty() {
                            text(&definition, "name")
                        } else {
                            custom
                        },
                        &id[id.len().saturating_sub(6)..]
                    ),
                )
            })
            .collect::<Vec<_>>();
        let options = copies
            .iter()
            .map(|(id, label)| (id.as_str(), label.as_str()))
            .collect::<Vec<_>>();
        self.ui.choice(
            "build:instance",
            self.ui.rect(x, 113.0, width.min(300.0), 31.0),
            &self.builds.instance,
            &options,
        );
        let configs = entries(&instance["configs"]);
        let mut cx = x;
        let config_width = (width / configs.len().max(1) as f32 - 6.0).min(92.0);
        for config in configs {
            let index = number(config, "config_index") as usize;
            let name = text(config, "name");
            let label = if name.is_empty() {
                format!(
                    "Config {}",
                    char::from_u32(65 + index as u32).unwrap_or('?')
                )
            } else {
                name.into()
            };
            self.ui.button(
                format!("build:config:{index}"),
                self.ui.rect(cx, 153.0, config_width, 30.0),
                &label,
                self.builds.config == index,
                true,
            );
            cx += config_width + 6.0;
        }
        self.ui.button(
            "build:new",
            self.ui.rect(x, 195.0, 125.0, 31.0),
            "Create group",
            true,
            !instance.is_null(),
        );
        self.ui.button(
            "build:add-config",
            self.ui.rect(x + 134.0, 195.0, 150.0, 31.0),
            "Add configuration",
            false,
            !instance.is_null(),
        );
        self.ui.button(
            "build:find",
            self.ui.rect(x + 293.0, 195.0, 110.0, 31.0),
            "Find builds",
            false,
            true,
        );
        let viewport = self.ui.rect(x, 240.0, width, (h - 240.0).max(0.0));
        self.ui.begin_scroll(viewport, self.builds.offset);
        let config = self.builds.config();
        let end = topology::render(
            &mut self.ui,
            &mut self.builds.preview,
            &instance["topology"],
            &config["upgrade_slots"],
            &instance["effective_polarities"],
            &instance["shard_slots"],
            &config["ability_override"],
            width,
            16.0,
        );
        self.finish_build_scroll(viewport, end);
    }
    fn build_discover(&mut self, w: f32, h: f32) {
        self.ui.field(
            "build:query",
            self.ui.rect(14.0, 78.0, w - 305.0, 33.0),
            &self.builds.query,
            "Find equipment or build",
        );
        self.ui.choice(
            "build:scope",
            self.ui.rect(w - 282.0, 78.0, 165.0, 33.0),
            &self.builds.scope,
            &[
                ("public", "Public builds"),
                ("favorites", "My favorites"),
                ("user", "My builds"),
            ],
        );
        self.ui.button(
            "build:search-source",
            self.ui.rect(w - 107.0, 78.0, 93.0, 33.0),
            "Search",
            true,
            true,
        );
        self.ui.choice(
            "build:ordering",
            self.ui.rect(14.0, 121.0, 150.0, 30.0),
            &self.builds.ordering,
            &[
                ("score", "Top rated"),
                ("updated_at", "Recently updated"),
                ("created_at", "Newest"),
            ],
        );
        if !self.builds.detail.is_null() {
            let detail = self.builds.detail.clone();
            let metadata = &detail["metadata"];
            self.ui.button(
                "build:back",
                self.ui.rect(w - 102.0, 121.0, 88.0, 30.0),
                "Back",
                false,
                true,
            );
            if !self.builds.added_group.is_empty() {
                self.ui.button(
                    "build:open-added",
                    self.ui.rect(w - 222.0, 121.0, 110.0, 30.0),
                    "Open group",
                    false,
                    true,
                );
            }
            self.ui.bold(
                self.ui.rect(14.0, 164.0, w - 28.0, 35.0),
                text(metadata, "title"),
                20.0,
                TEXT,
            );
            self.ui.text(
                self.ui.rect(14.0, 202.0, w - 290.0, 27.0),
                format!(
                    "{}  -  {:.0} Forma",
                    text(&metadata["author"], "username"),
                    number(metadata, "formas")
                ),
                13.0,
                MUTED,
            );
            self.ui.button(
                "build:add-source",
                self.ui.rect(w - 264.0, 201.0, 145.0, 31.0),
                "Add to group",
                true,
                true,
            );
            self.ui.button(
                "build:website",
                self.ui.rect(w - 109.0, 201.0, 95.0, 31.0),
                "Overframe",
                false,
                true,
            );
            let viewport = self.ui.rect(10.0, 244.0, w - 20.0, (h - 244.0).max(0.0));
            self.ui.begin_scroll(viewport, self.builds.offset);
            let mut end = topology::render(
                &mut self.ui,
                &mut self.builds.preview,
                &detail["presentation"]["topology"],
                &detail["presentation"]["upgrades"],
                &Value::Null,
                &Value::Null,
                &detail["content"]["ability_override"],
                w - 20.0,
                16.0,
            );
            end = self.builds.notes.render(
                &mut self.ui,
                text(metadata, "description"),
                w - 20.0,
                end + 16.0,
            );
            self.finish_build_scroll(viewport, end);
            return;
        }
        let rows = if self.builds.sources.is_null() {
            entries(&self.builds.items["items"])
        } else {
            entries(&self.builds.sources["builds"])
        }
        .to_vec();
        let viewport = self.ui.rect(12.0, 164.0, w - 24.0, (h - 209.0).max(0.0));
        self.ui.begin_scroll(viewport, self.builds.offset);
        for (i, row) in rows.iter().enumerate() {
            let y = i as f32 * 74.0;
            if self.ui.px(y + 74.0) < self.builds.offset
                || self.ui.px(y) > self.builds.offset + viewport.height
            {
                continue;
            }
            let is_build = row["identity"].is_object();
            let label = text(row, if is_build { "title" } else { "name" });
            let id = if is_build {
                row["identity"]["external_id"].to_string()
            } else {
                text(row, "canonical_id").into()
            };
            self.ui
                .fill(self.ui.rect(0.0, y, w - 24.0, 67.0), 6.0, widgets::CARD);
            self.ui.bold(
                self.ui.rect(12.0, y + 5.0, w - 48.0, 28.0),
                label,
                16.0,
                TEXT,
            );
            self.ui.text(
                self.ui.rect(12.0, y + 34.0, w - 48.0, 24.0),
                if is_build {
                    format!(
                        "{}  -  {:.0} Forma  -  Score {:.0}",
                        text(&row["author"], "username"),
                        number(row, "formas"),
                        number(row, "score")
                    )
                } else {
                    text(row, "class").to_owned()
                },
                13.0,
                MUTED,
            );
            self.ui.hit(
                format!(
                    "build:{}:{id}",
                    if is_build { "detail" } else { "source-item" }
                ),
                self.ui.rect(0.0, y, w - 24.0, 67.0),
                label,
                true,
            );
        }
        self.finish_build_scroll(viewport, rows.len() as f32 * 74.0);
        if rows.is_empty() {
            self.ui.text(
                self.ui.rect(24.0, 196.0, w - 48.0, 40.0),
                if self
                    .pending
                    .keys()
                    .any(|k| k.starts_with("build:search") || k == "build:list")
                {
                    "Loading builds..."
                } else {
                    "Search equipment to browse builds"
                },
                16.0,
                MUTED,
            );
        }
        if !self.builds.sources.is_null() {
            self.ui.button(
                "build:previous",
                self.ui.rect(14.0, h - 39.0, 94.0, 30.0),
                "Previous",
                false,
                self.builds.source_offset > 0,
            );
            self.ui.button(
                "build:next",
                self.ui.rect(w - 108.0, h - 39.0, 94.0, 30.0),
                "Next",
                false,
                rows.len() >= 30,
            );
        }
    }
    fn build_groups(&mut self, w: f32, h: f32) {
        let groups = entries(&self.builds.groups["groups"]);
        if groups.is_empty() {
            let top = 60.0 + (h - 60.0 - 132.0).max(0.0) / 2.0;
            self.ui.aligned(
                self.ui.rect(30.0, top, w - 60.0, 38.0),
                if self.builds.groups.is_null() {
                    "Loading build groups..."
                } else {
                    "No build groups"
                },
                24.0,
                TEXT,
                widgets::Weight::Regular,
                0.5,
            );
            self.ui.aligned(
                self.ui.rect(30.0, top + 48.0, w - 60.0, 36.0),
                "Choose equipment and add the builds you want it to support.",
                15.0,
                MUTED,
                widgets::Weight::Regular,
                0.5,
            );
            self.ui.button(
                "build:tab:equipment",
                self.ui.rect((w - 332.0) / 2.0, top + 96.0, 170.0, 36.0),
                "Choose equipment",
                true,
                true,
            );
            self.ui.button(
                "build:tab:discover",
                self.ui
                    .rect((w - 332.0) / 2.0 + 182.0, top + 96.0, 150.0, 36.0),
                "Discover builds",
                false,
                true,
            );
            return;
        }
        let options = groups
            .iter()
            .map(|g| (text(g, "id"), text(g, "name")))
            .collect::<Vec<_>>();
        let selected = text(&self.builds.group, "id");
        self.ui.choice(
            "build:group",
            self.ui.rect(14.0, 78.0, w - 300.0, 32.0),
            selected,
            &options,
        );
        self.ui.button(
            "build:rename",
            self.ui.rect(w - 276.0, 78.0, 90.0, 32.0),
            "Rename",
            false,
            !selected.is_empty(),
        );
        self.ui.destructive(
            "build:delete",
            self.ui.rect(w - 176.0, 78.0, 80.0, 32.0),
            "Delete",
            !selected.is_empty(),
        );
        self.ui.button(
            "build:calculate",
            self.ui.rect(w - 86.0, 78.0, 72.0, 32.0),
            if self.pending.contains_key("build:plan") {
                "Working..."
            } else {
                "Calculate"
            },
            true,
            !selected.is_empty() && !self.build_busy(),
        );
        if self.builds.group.is_null() {
            self.ui.text(
                self.ui.rect(24.0, 132.0, w - 48.0, 32.0),
                "Choose a group",
                16.0,
                MUTED,
            );
            return;
        }
        let group = self.builds.group.clone();
        let copies = entries(&self.builds.equipment["instances"])
            .iter()
            .filter(|i| i["definition_id"] == group["definition_id"])
            .map(|i| {
                let id = text(i, "instance_id");
                let name = text(i, "custom_name");
                (
                    id.to_owned(),
                    format!(
                        "{} ({})",
                        if name.is_empty() { "Owned copy" } else { name },
                        id.chars()
                            .rev()
                            .take(6)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect::<String>()
                    ),
                )
            })
            .collect::<Vec<_>>();
        let mut copy_refs = copies
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect::<Vec<_>>();
        if text(&group, "instance_id").is_empty() {
            copy_refs.insert(0, ("", "Choose an owned copy"));
        }
        self.ui.choice(
            "build:group-instance",
            self.ui.rect(14.0, 119.0, (w - 28.0).min(420.0), 30.0),
            text(&group, "instance_id"),
            &copy_refs,
        );
        let options = &group["options"];
        for (i, (key, label)) in [
            ("preserve_source_slots", "Keep exact slots"),
            ("allow_omni", "Allow Omni Forma"),
            ("allow_umbral_forma", "Allow Umbral Forma"),
        ]
        .into_iter()
        .enumerate()
        {
            self.ui.toggle(
                format!("build:option:{key}"),
                self.ui.rect(
                    14.0 + i as f32 * (w - 28.0) / 3.0,
                    157.0,
                    (w - 28.0) / 3.0,
                    28.0,
                ),
                label,
                options[key] == true,
            );
        }
        let members = entries(&group["members"]);
        let member = members
            .iter()
            .find(|m| text(m, "id") == self.builds.member)
            .or_else(|| members.first())
            .cloned()
            .unwrap_or(Value::Null);
        let choices = members
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    text(m, "id").to_owned(),
                    format!("{}. {}", i + 1, text(m, "name")),
                )
            })
            .collect::<Vec<_>>();
        let refs = choices
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect::<Vec<_>>();
        self.ui.choice(
            "build:member",
            self.ui.rect(14.0, 195.0, w - 234.0, 33.0),
            text(&member, "id"),
            &refs,
        );
        self.ui.button(
            "build:remove",
            self.ui.rect(w - 210.0, 195.0, 90.0, 33.0),
            "Remove",
            false,
            !member.is_null(),
        );
        self.ui.button(
            "build:planned",
            self.ui.rect(w - 110.0, 195.0, 96.0, 33.0),
            if self.builds.planned {
                "Planned"
            } else {
                "Original"
            },
            self.builds.planned,
            text(&group["plan_result"], "status") == "ready",
        );
        self.ui.button(
            "build:add-config",
            self.ui.rect(14.0, 240.0, 160.0, 31.0),
            "Add configuration",
            false,
            true,
        );
        self.ui.button(
            "build:group-find",
            self.ui.rect(184.0, 240.0, 120.0, 31.0),
            "Find builds",
            false,
            true,
        );
        let viewport = self.ui.rect(10.0, 285.0, w - 20.0, (h - 285.0).max(0.0));
        self.ui.begin_scroll(viewport, self.builds.offset);
        let baseline = &group["baseline"];
        let result = &group["plan_result"];
        let mut y = 12.0;
        if text(result, "status") == "ready" {
            let operations = entries(&result["operations"]);
            self.ui.bold(
                self.ui.rect(14.0, y, w - 48.0, 28.0),
                if operations.is_empty() {
                    "Current polarities support every build"
                } else {
                    "Polarity changes"
                },
                17.0,
                GOOD,
            );
            y += 34.0;
            y = topology::plan_polarities(
                &mut self.ui,
                &baseline["topology"],
                &result["final_polarities"],
                w - 20.0,
                y,
            );
            for (i, op) in operations.iter().enumerate() {
                let description = if text(op, "action") == "swap" {
                    format!(
                        "{}. Swap {} with {}",
                        i + 1,
                        text(op, "label"),
                        text(op, "other_label")
                    )
                } else {
                    format!(
                        "{}. Apply {} Forma to {}: {} -> {}",
                        i + 1,
                        text(op, "forma"),
                        text(op, "label"),
                        text(op, "before"),
                        text(op, "polarity")
                    )
                };
                self.ui.text(
                    self.ui.rect(14.0, y, w - 48.0, 27.0),
                    description,
                    14.0,
                    TEXT,
                );
                y += 29.0;
            }
            let resources = result["forma_requirements"]
                .as_object()
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| {
                            v.as_u64()
                                .filter(|n| *n > 0)
                                .map(|n| format!("{n} {}", k.replace('_', " ")))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            if !resources.is_empty() {
                self.ui.text(
                    self.ui.rect(14.0, y, w - 48.0, 28.0),
                    resources,
                    14.0,
                    MUTED,
                );
                y += 34.0;
            }
        } else if text(result, "status") == "blocked" {
            self.ui.text(
                self.ui.rect(14.0, y, w - 48.0, 32.0),
                format!("Plan blocked: {}", text(result, "reason").replace('_', " ")),
                15.0,
                BAD,
            );
            y += 42.0;
        }
        let snapshot = &member["snapshot"];
        let planned = entries(&result["builds"])
            .iter()
            .find(|b| text(b, "member_id") == text(&member, "id"));
        if let Some(plan) = planned {
            self.ui.text(
                self.ui.rect(14.0, y, w - 48.0, 28.0),
                format!(
                    "Planned capacity: {:.0} / {:.0} used, {:.0} remaining",
                    number(plan, "drain"),
                    number(plan, "capacity"),
                    number(plan, "remaining_capacity")
                ),
                14.0,
                if number(plan, "remaining_capacity") >= 0.0 {
                    GOOD
                } else {
                    BAD
                },
            );
            y += 34.0;
        }
        let (topology, upgrades, polarities, overrides) =
            if self.builds.planned && planned.is_some() {
                let plan = planned.unwrap();
                (
                    &baseline["topology"],
                    &plan["upgrade_slots"],
                    &result["final_polarities"],
                    &plan["ability_override"],
                )
            } else if text(&member, "kind") == "player_config" {
                (
                    &baseline["topology"],
                    &snapshot["config"]["upgrade_slots"],
                    &baseline["effective_polarities"],
                    &snapshot["config"]["ability_override"],
                )
            } else {
                (
                    &snapshot["presentation"]["topology"],
                    &snapshot["presentation"]["upgrades"],
                    &baseline["effective_polarities"],
                    &snapshot["content"]["ability_override"],
                )
            };
        if member.is_null() {
            self.ui.text(
                self.ui.rect(16.0, y, w - 52.0, 40.0),
                "Add a configuration or find a build for this group",
                16.0,
                MUTED,
            );
            y += 60.0;
        } else {
            y = topology::render(
                &mut self.ui,
                &mut self.builds.preview,
                topology,
                upgrades,
                polarities,
                &baseline["shard_slots"],
                overrides,
                w - 20.0,
                y + 14.0,
            );
        }
        y = self.builds.notes.render(
            &mut self.ui,
            text(&snapshot["metadata"], "description"),
            w - 20.0,
            y + 16.0,
        );
        self.finish_build_scroll(viewport, y);
    }
    fn build_dialog(&mut self, w: f32, h: f32) {
        let Some(kind) = self.builds.dialog.clone() else {
            return;
        };
        self.ui.begin_modal();
        let x = (w - 380.0) / 2.0;
        let y = (h - 238.0) / 2.0;
        self.ui
            .fill(self.ui.rect(0.0, 0.0, w, h), 0.0, [0, 0, 0, 145]);
        self.ui
            .fill(self.ui.rect(x, y, 380.0, 238.0), 8.0, widgets::POPOVER);
        let title = match kind.as_str() {
            "create" | "create-source" => "Create build group",
            "rename" => "Rename group",
            "delete" => "Delete this group?",
            "source" => "Add build to group",
            _ => "Add current configuration",
        };
        self.ui.bold(
            self.ui.rect(x + 16.0, y + 15.0, 348.0, 34.0),
            title,
            20.0,
            TEXT,
        );
        if matches!(kind.as_str(), "create" | "create-source" | "rename") {
            self.ui.field(
                "build:name",
                self.ui.rect(x + 16.0, y + 65.0, 348.0, 36.0),
                &self.builds.name,
                "Group name",
            );
        } else if matches!(kind.as_str(), "source" | "config") {
            let definition = if kind == "source" {
                text(&self.builds.detail["metadata"]["item"], "canonical_id")
            } else {
                &self.builds.definition
            };
            let options = entries(&self.builds.groups["groups"])
                .iter()
                .filter(|g| text(g, "definition_id") == definition)
                .map(|g| (text(g, "id"), text(g, "name")))
                .collect::<Vec<_>>();
            self.ui.choice(
                "build:target",
                self.ui.rect(x + 16.0, y + 65.0, 348.0, 36.0),
                &self.builds.target,
                &options,
            );
            if options.is_empty() {
                self.ui.text(
                    self.ui.rect(x + 16.0, y + 111.0, 348.0, 42.0),
                    "Create a group for this equipment first",
                    14.0,
                    MUTED,
                );
            }
            if kind == "source" {
                self.ui.button(
                    "build:new-source",
                    self.ui.rect(x + 16.0, y + 126.0, 160.0, 30.0),
                    "New group...",
                    false,
                    true,
                );
            }
        } else {
            self.ui.text(
                self.ui.rect(x + 16.0, y + 70.0, 348.0, 55.0),
                "The saved group and its plan will be removed.",
                14.0,
                MUTED,
            );
        }
        self.ui.button(
            "build:cancel",
            self.ui.rect(x + 16.0, y + 182.0, 162.0, 36.0),
            "Cancel",
            false,
            true,
        );
        let valid = match kind.as_str() {
            "create" | "create-source" | "rename" => !self.builds.name.trim().is_empty(),
            "source" | "config" => !self.builds.target.is_empty(),
            _ => true,
        };
        self.ui.button(
            "build:confirm",
            self.ui.rect(x + 196.0, y + 182.0, 168.0, 36.0),
            "Confirm",
            true,
            valid && !self.build_busy(),
        );
    }
    pub(super) fn build_reply(&mut self, request: &Request, data: Value) {
        match request.key.as_str() {
            "build:search" => {
                self.builds.items = data;
                self.builds.sources = Value::Null;
                self.builds.detail = Value::Null;
            }
            "build:list" => {
                self.builds.sources = data;
                self.builds.detail = Value::Null;
            }
            "build:detail" => self.builds.detail = data,
            "build:group" => {
                if data["id"] == self.builds.group["id"]
                    && number(&data, "revision") < number(&self.builds.group, "revision")
                {
                    return;
                }
                self.builds.group = data;
                if !entries(&self.builds.group["members"])
                    .iter()
                    .any(|m| text(m, "id") == self.builds.member)
                {
                    self.builds.member = entries(&self.builds.group["members"])
                        .first()
                        .map(|m| text(m, "id").to_owned())
                        .unwrap_or_default();
                }
            }
            "build:plan" => {
                self.builds.planned = true;
                let id = text(&self.builds.group, "id").to_owned();
                self.queue("build:group", json!({"op":"build_group_get","group_id":id}));
            }
            _ if request.key.starts_with("build:write") => {
                if matches!(
                    request.body["op"].as_str(),
                    Some("overframe_session_import" | "overframe_logout")
                ) {
                    self.queue("overframe", json!({"op":"overframe_account"}));
                    return;
                }
                self.builds.dialog = None;
                self.builds.notice = Some("Saved".into());
                if request.key == "build:write:create-source" {
                    self.queue("build:write:add",json!({"op":"build_group_add_source","group_id":data["id"],"revision":data["revision"],"source":"overframe","external_id":self.builds.detail["identity"]["external_id"],"fingerprint":self.builds.detail["fingerprint"]}));
                }
                if request.body["op"] == "build_group_add_source" {
                    self.builds.added_group = text(&data, "id").into();
                    self.builds.notice = Some("Build added to group".into());
                }
                if text(&data, "id") == text(&self.builds.group, "id")
                    || request.body["op"] == "build_group_create"
                {
                    self.builds.group = data.clone();
                }
                self.queue("groups", json!({"op":"build_group_list"}));
                let id = text(&self.builds.group, "id").to_owned();
                if !id.is_empty()
                    && request.body["op"] != "build_group_delete"
                    && request.key != "build:write:create-source"
                {
                    self.queue("build:group", json!({"op":"build_group_get","group_id":id}));
                }
                if request.body["op"] == "build_group_delete" {
                    self.builds.group = Value::Null;
                }
            }
            _ => (),
        }
    }
    fn request_build_list(&mut self) {
        self.queue("build:list",json!({"op":"build_list","item":self.builds.definition,"query":self.builds.query,"scope":self.builds.scope,"ordering":self.builds.ordering,"limit":30,"offset":self.builds.source_offset,"refresh":false}));
    }
    pub(super) fn build_signal(&mut self, signal: Signal) {
        let key = signal.key.as_str();
        let value = signal.value.unwrap_or_default();
        if self.build_busy()
            && (matches!(
                key,
                "build:confirm" | "build:remove" | "build:calculate" | "build:group-instance"
            ) || key.starts_with("build:option:"))
        {
            return;
        }
        self.builds.notice = None;
        match key{
            "build:list-scroll"=>{if let Ok(offset)=value.parse(){self.builds.list_scroll.to(offset,false,Instant::now());self.builds.tick(Instant::now());}},
            "build:scroll"=>{if let Ok(offset)=value.parse(){self.builds.scroll.to(offset,false,Instant::now());self.builds.tick(Instant::now());}},
            "build:search"=>{self.builds.search=value;self.builds.list_scroll.to(0.0,false,Instant::now());},
            "build:class"=>{self.builds.class=value;self.builds.list_scroll.to(0.0,false,Instant::now());},
            "build:instance"=>{self.builds.instance=value;self.builds.config=0;},
            "build:collapse"=>{self.builds.collapsed=!self.builds.collapsed;self.builds.collapse.set(if self.builds.collapsed{1.0}else{0.0},Instant::now(),Duration::from_millis(170));},
            "build:new"=>{self.builds.name=text(&self.builds.definition(),"name").into();self.builds.dialog=Some("create".into());},
            "build:new-source"=>{self.builds.name=format!("{} builds",text(&self.builds.detail["metadata"]["item"],"name"));self.builds.dialog=Some("create-source".into());},
            "build:open-added"=>{self.builds.tab="groups".into();self.queue("build:group",json!({"op":"build_group_get","group_id":self.builds.added_group}));self.builds.scroll.to(0.0,false,Instant::now());},
            "build:group-instance"=>self.queue("build:write:instance",json!({"op":"build_group_update","group_id":self.builds.group["id"],"revision":self.builds.group["revision"],"patch":{"instance_id":value}})),
            "build:rename"=>{self.builds.name=text(&self.builds.group,"name").into();self.builds.dialog=Some("rename".into());},
            "build:delete"=>self.builds.dialog=Some("delete".into()),
            "build:name"=>self.builds.name=value,
            "build:target"=>self.builds.target=value,
            "build:cancel"=>self.builds.dialog=None,
            "build:find"|"build:group-find"=>{
                if key=="build:group-find"{self.builds.definition=text(&self.builds.group,"definition_id").into();}
                self.builds.tab="discover".into();self.builds.detail=Value::Null;self.builds.query.clear();self.builds.source_offset=0;self.request_build_list();
            }
            "build:query"=>self.builds.query=value,
            "build:query.submit"|"build:search-source"=>{
                if self.builds.scope=="public"{self.queue("build:search",json!({"op":"build_search","query":self.builds.query,"class":"all","limit":60}));}
                else{self.builds.definition.clear();self.builds.source_offset=0;self.request_build_list();}
            }
            "build:scope"=>self.builds.scope=value,
            "build:ordering"=>{self.builds.ordering=value;self.builds.source_offset=0;if !self.builds.sources.is_null(){self.request_build_list();}},
            "build:next"=>{self.builds.source_offset+=30;self.request_build_list();self.builds.scroll.to(0.0,false,Instant::now());},
            "build:previous"=>{self.builds.source_offset=self.builds.source_offset.saturating_sub(30);self.request_build_list();self.builds.scroll.to(0.0,false,Instant::now());},
            "build:back"=>{self.builds.detail=Value::Null;self.builds.scroll.to(0.0,false,Instant::now());},
            "build:website"=>{let url=text(&self.builds.detail["metadata"],"url");if url.starts_with("/build/"){self.external=Some(format!("https://overframe.gg{url}"));}},
            _ if key.starts_with("build:link:")=>{self.external=notes::link_url(key.trim_start_matches("build:link:"));},
            "build:account"=>{
                if self.builds.account["authenticated"]==true{self.queue("build:write:logout",json!({"op":"overframe_logout"}));self.queue("overframe",json!({"op":"overframe_account"}));}
                else{self.login_requested=true;}
            }
            "build:group"=>{self.queue("build:group",json!({"op":"build_group_get","group_id":value}));self.builds.scroll.to(0.0,false,Instant::now());},
            "build:member"=>{self.builds.member=value;self.builds.scroll.to(0.0,false,Instant::now());},
            "build:planned"=>self.builds.planned=!self.builds.planned,
            "build:calculate"=>self.queue("build:plan",json!({"op":"build_group_plan","group_id":self.builds.group["id"],"revision":self.builds.group["revision"]})),
            "build:remove"=>{
                let id=if self.builds.member.is_empty(){entries(&self.builds.group["members"]).first().map(|m|text(m,"id")).unwrap_or("").to_owned()}else{self.builds.member.clone()};
                self.queue("build:write:remove",json!({"op":"build_group_remove_member","group_id":self.builds.group["id"],"revision":self.builds.group["revision"],"member_id":id}));
            }
            "build:add-config"|"build:add-source"=>{
                if key=="build:add-config"&&self.builds.tab=="groups"{
                    self.builds.definition=text(&self.builds.group,"definition_id").into();self.builds.instance=text(&self.builds.group,"instance_id").into();self.builds.tab="equipment".into();return;
                }
                let definition=if key=="build:add-source"{text(&self.builds.detail["metadata"]["item"],"canonical_id")}else{&self.builds.definition};
                self.builds.target=entries(&self.builds.groups["groups"]).iter().find(|g|text(g,"definition_id")==definition).map(|g|text(g,"id").to_owned()).unwrap_or_default();
                self.builds.dialog=Some(if key=="build:add-source"{"source"}else{"config"}.into());
            }
            "build:confirm"=>self.confirm_build(),
            _ if key.starts_with("build:option:")=>{
                let option=key.trim_start_matches("build:option:");let mut options=self.builds.group["options"].clone();options[option]=json!(options[option]!=true);
                self.queue("build:write:options",json!({"op":"build_group_update","group_id":self.builds.group["id"],"revision":self.builds.group["revision"],"patch":{"options":options}}));
            }
            _ if key.starts_with("build:tab:")=>{self.builds.tab=key.trim_start_matches("build:tab:").into();self.builds.scroll.to(0.0,false,Instant::now());},
            _ if key.starts_with("build:equipment:")=>{
                self.builds.definition=key.trim_start_matches("build:equipment:").into();
                self.builds.instance=entries(&self.builds.equipment["instances"]).iter().find(|i|text(i,"definition_id")==self.builds.definition).map(|i|text(i,"instance_id").to_owned()).unwrap_or_default();self.builds.config=0;self.builds.scroll.to(0.0,false,Instant::now());
            }
            _ if key.starts_with("build:config:")=>self.builds.config=key.trim_start_matches("build:config:").parse().unwrap_or(0),
            _ if key.starts_with("build:source-item:")=>{self.builds.definition=key.trim_start_matches("build:source-item:").into();self.builds.query.clear();self.builds.source_offset=0;self.request_build_list();self.builds.scroll.to(0.0,false,Instant::now());},
            _ if key.starts_with("build:detail:")=>{
                if let Ok(id)=key.trim_start_matches("build:detail:").parse::<u64>(){self.queue("build:detail",json!({"op":"build_detail","build_id":id,"refresh":false}));self.builds.scroll.to(0.0,false,Instant::now());}
            }
            _=>(),
        }
    }
    fn confirm_build(&mut self) {
        let group = self.builds.group.clone();
        match self.builds.dialog.as_deref(){
            Some("create-source") if !self.builds.name.trim().is_empty()=>self.queue("build:write:create-source",json!({"op":"build_group_create","group":{"name":self.builds.name.trim(),"definition_id":self.builds.detail["metadata"]["item"]["canonical_id"]}})),
            Some("create") if !self.builds.name.trim().is_empty()=>self.queue("build:write:create",json!({"op":"build_group_create","group":{"name":self.builds.name.trim(),"definition_id":self.builds.definition,"instance_id":self.builds.instance}})),
            Some("rename") if !self.builds.name.trim().is_empty()=>self.queue("build:write:rename",json!({"op":"build_group_update","group_id":group["id"],"revision":group["revision"],"patch":{"name":self.builds.name.trim()}})),
            Some("delete")=>self.queue("build:write:delete",json!({"op":"build_group_delete","group_id":group["id"],"revision":group["revision"]})),
            Some("source"|"config")=>{
                if let Some(target)=entries(&self.builds.groups["groups"]).iter().find(|g|text(g,"id")==self.builds.target){
                    let mut request=json!({"group_id":target["id"],"revision":target["revision"]});
                    if self.builds.dialog.as_deref()==Some("source"){
                        request["op"]=json!("build_group_add_source");request["source"]=json!("overframe");request["external_id"]=self.builds.detail["identity"]["external_id"].clone();request["fingerprint"]=self.builds.detail["fingerprint"].clone();
                    }else{request["op"]=json!("build_group_add_config");request["instance_id"]=json!(self.builds.instance);request["config_index"]=json!(self.builds.config);}
                    self.queue("build:write:add",request);
                }
            }
            _=>(),
        }
    }
}
