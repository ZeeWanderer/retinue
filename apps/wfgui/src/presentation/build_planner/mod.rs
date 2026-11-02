mod fixtures;
mod notes;
#[cfg(test)]
mod state_tests;
mod topology;

use super::*;
use crate::presentation::settings::rgb;
use controls::{ACCENT, BAD, GOOD, MUTED, TEXT};
use retinue_ui::transition::ScalarTransition;

pub struct Builds {
    pub equipment: Option<retinue_protocol::warframe::Equipment>,
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
    revision: u64,
}
impl Default for Builds {
    fn default() -> Self {
        Self {
            equipment: None,
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
            revision: 0,
        }
    }
}
impl Builds {
    pub(super) fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
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
        self.definitions()
            .iter()
            .find(|d| d.id == self.definition)
            .map(|d| serde_json::to_value(d).expect("equipment definition"))
            .unwrap_or(Value::Null)
    }
    fn definitions(&self) -> &[retinue_protocol::warframe::EquipmentDefinition] {
        self.equipment.as_ref().map_or(&[], |e| &e.definitions)
    }
    fn instances(&self) -> &[retinue_protocol::warframe::EquipmentInstance] {
        self.equipment.as_ref().map_or(&[], |e| &e.instances)
    }
    fn instance(&self) -> Value {
        self.instances()
            .iter()
            .find(|i| i.instance_id == self.instance)
            .map(|i| serde_json::to_value(i).expect("selected equipment"))
            .unwrap_or(Value::Null)
    }
    fn config(&self) -> Value {
        self.instances()
            .iter()
            .find(|i| i.instance_id == self.instance)
            .and_then(|i| {
                i.configs
                    .iter()
                    .find(|c| c.config_index as usize == self.config)
            })
            .map(|c| serde_json::to_value(c).expect("selected equipment config"))
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
        self.builds.invalidate();
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
                let item = self
                    .builds
                    .definitions()
                    .iter()
                    .find(|v| v.id == select || v.name.eq_ignore_ascii_case(select))
                    .ok_or_else(|| format!("Equipment not found: {select}"))?;
                let id = item.id.clone();
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
    pub(super) fn build_reply(&mut self, request: &Request, data: Value) {
        self.builds.invalidate();
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
        if matches!(key, "build:find" | "build:group-find" | "build:back") {
            self.builds.invalidate();
        }
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
                self.builds.instance=self.builds.instances().iter().find(|i|i.definition_id==self.builds.definition).map(|i|i.instance_id.clone()).unwrap_or_default();self.builds.config=0;self.builds.scroll.to(0.0,false,Instant::now());
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

mod build_planner_model;
mod build_planner_view;
mod content;
mod dialog;
mod discover;
mod equipment;
mod groups;
#[cfg(test)]
mod tests;

use crate::{
    presentation::widgets::Fonts, presentation::widgets::fill, presentation::widgets::fitted_label,
    presentation::widgets::key, presentation::widgets::place, presentation::widgets::slot,
};
use retinue_render::{Bounds, CacheHint, Key};
use retinue_ui::{layout::anchors::AnchorPoint, prelude::*, semantics::Role};
use std::rc::Rc;

const SCROLL: Key = Key::new("build.scroll", 0);
const BODY: Key = Key::new("build.body", 0);

#[derive(Clone, Default, PartialEq)]
struct Metrics {
    width: f32,
    height: f32,
    scale: f32,
    sidebar: f32,
    tabs: [f32; 3],
}

impl Metrics {
    fn bounds(&self, x: f32, y: f32, w: f32, h: f32) -> Bounds {
        Bounds::new(x.max(0.0), y.max(0.0), w.max(0.0), h.max(0.0))
    }
    fn local(&self, parent: Bounds, child: Bounds) -> Bounds {
        let r = self.bounds(
            parent.x + child.x,
            parent.y + child.y,
            child.width,
            child.height,
        );
        Bounds::new(r.x - parent.x, r.y - parent.y, r.width, r.height)
    }
    fn viewport(&self, tab: &str) -> Bounds {
        match tab {
            "equipment" => self.bounds(
                self.sidebar + 10.0,
                240.0,
                self.width - self.sidebar - 22.0,
                self.height - 240.0,
            ),
            "discover" => self.bounds(10.0, 244.0, self.width - 20.0, self.height - 244.0),
            _ => self.bounds(10.0, 285.0, self.width - 20.0, self.height - 285.0),
        }
    }
    fn body_width(&self, tab: &str) -> f32 {
        if tab == "equipment" {
            self.width - self.sidebar - 22.0
        } else {
            self.width - 20.0
        }
    }
    fn equipment_list(&self) -> Bounds {
        self.bounds(7.0, 153.0, self.sidebar - 14.0, self.height - 153.0)
    }
    fn source_list(&self) -> Bounds {
        self.bounds(12.0, 164.0, self.width - 24.0, self.height - 209.0)
    }
}

#[derive(Clone, Default, PartialEq)]
struct PageState {
    tab: String,
    account: String,
    busy: bool,
    planning: bool,
    searching: bool,
    added: bool,
    error: Option<String>,
    notice: Option<String>,
}

#[derive(PartialEq)]
struct Projection {
    revision: u64,
    definition: String,
    instance: String,
    config: usize,
    member: String,
    planned: bool,
}

pub(in crate::presentation) struct BuildPlannerController {
    metrics: State<Metrics>,
    state: State<PageState>,
    model: State<Rc<build_planner_model::Model>>,
    equipment: equipment::List,
    sources: discover::List,
    content: content::Content,
    dialog: dialog::Dialog,
    projection: Option<Projection>,
    longest: f32,
    pub installed: bool,
}

fn node(name: &'static str) -> Key {
    key("build.header", name)
}
fn control(id: Key, rect: Bounds) -> Element {
    place(slot(id), rect).interactive()
}
fn label(fonts: &Fonts, text: &str, size: f32, color: [u8; 4], bold: bool, scale: f32) -> Element {
    fitted_label(
        text,
        if bold {
            fonts.bold.clone()
        } else {
            fonts.regular.clone()
        },
        size,
        color,
        false,
        scale,
    )
}
fn choices(options: &[(String, String)]) -> Vec<(&str, &str)> {
    options
        .iter()
        .map(|(id, name)| (id.as_str(), name.as_str()))
        .collect()
}

fn visible_rows(offset: f32, height: f32, stride: f32, count: usize) -> std::ops::Range<usize> {
    let first = ((offset / stride).ceil() as usize)
        .saturating_sub(1)
        .min(count);
    let end = ((offset + height) / stride).floor() as usize + 1;
    first..end.min(count)
}

impl BuildPlannerController {
    pub fn new() -> Self {
        Self {
            metrics: State::new(Metrics {
                scale: 1.0,
                ..Default::default()
            }),
            state: State::default(),
            model: State::new(Rc::new(build_planner_model::Model::default())),
            equipment: equipment::List::new(),
            sources: discover::List::new(),
            content: content::Content::new(),
            dialog: dialog::Dialog::new(),
            projection: None,
            longest: 195.0,
            installed: false,
        }
    }

    fn view(&self, ui: &ControlHost) -> Element {
        let fonts = Fonts::new(ui);
        build_planner_view::BuildPageView {
            metrics: self.metrics.clone(),
            state: self.state.clone(),
            fonts: fonts.clone(),
            equipment: self
                .equipment
                .view(self.metrics.clone(), self.model.clone(), fonts.clone()),
            discover: self.sources.view(
                self.metrics.clone(),
                self.model.clone(),
                self.state.clone(),
                fonts.clone(),
            ),
            groups: groups::groups_view::GroupsView {
                metrics: self.metrics.clone(),
                model: self.model.clone(),
                fonts: fonts.clone(),
            }
            .into_element(),
            content: self.content.view(self.metrics.clone(), self.state.clone()),
            previews: self.content.previews_view(self.metrics.clone()),
            dialog: self.dialog.view(self.metrics.clone(), fonts),
        }
        .into_element()
    }

    fn sync(&mut self, ui: &ControlHost, b: &mut Builds, state: PageState, now: Instant) {
        let projection = Projection {
            revision: b.revision,
            definition: b.definition.clone(),
            instance: b.instance.clone(),
            config: b.config,
            member: b.member.clone(),
            planned: b.planned,
        };
        let model_changed = self.projection.as_ref() != Some(&projection);
        if model_changed {
            self.model.set(Rc::new(build_planner_model::Model::new(b)));
            self.projection = Some(projection);
        }
        let scale_changed = self.metrics.get().scale != ui.scale;
        if model_changed || scale_changed {
            self.longest = b
                .definitions()
                .iter()
                .map(|d| {
                    retinue_render::painter::text_width(&ui.regular, &d.name, 13.0 * ui.scale)
                        / ui.scale
                        + 60.0
                })
                .fold(195.0, f32::max)
                .min(295.0);
        }
        let width = ui.size.0 as f32 / ui.scale;
        if b.tab == "equipment" && (width < 720.0) != b.narrow {
            b.narrow = width < 720.0;
            b.collapsed = b.narrow;
            b.collapse
                .set(if b.collapsed { 1.0 } else { 0.0 }, now, Duration::ZERO);
        }
        let m = Metrics {
            width,
            height: ui.size.1 as f32 / ui.scale,
            scale: ui.scale,
            sidebar: 40.0
                + (self.longest.min((width * 0.35).max(190.0)) - 40.0)
                    * (1.0 - b.collapse.value(now)),
            tabs: ["Groups", "Equipment", "Discover"]
                .map(|name| ui.measure(name, 14.0, false) + 18.0),
        };
        self.metrics.set(m.clone());
        self.state.set(state);
        let model = self.model.get();
        b.list_viewport = None;
        self.equipment
            .sync(&m, &model.equipment, b, model_changed, now);
        self.sources.sync(&m, &model.discover, b, now);
        self.content.sync(ui, &m, &model, b, now);
        self.dialog.sync(b);
    }

    fn bind(&self, ui: &mut ControlHost, b: &Builds) {
        let state = self.state.get();
        let model = self.model.get();
        ui.view_control(node("warning.icon"), |ui, r| {
            ui.bitmap(
                r,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/resources/assets/triangle-alert.png"
                )),
            )
        });
        ui.view_hit(
            node("warning"),
            "build:construction",
            "Under construction",
            false,
        );
        for (id, name) in [
            ("groups", "Groups"),
            ("equipment", "Equipment"),
            ("discover", "Discover"),
        ] {
            ui.view_control(node(id), |ui, r| {
                ui.chip(format!("build:tab:{id}"), r, name, b.tab == id)
            });
        }
        ui.view_control(node("account"), |ui, r| {
            ui.button("build:account", r, &state.account, false, true)
        });
        equipment::bind(ui, b, &model.equipment, &self.equipment);
        discover::bind(ui, b, &model.discover, &self.sources);
        groups::bind(ui, b, &model.groups, &state);
        self.content.bind(ui, b);
        self.dialog.bind(ui, b, state.busy);
    }
}

impl Pages {
    pub(in crate::presentation) fn layout_builds(&mut self) {
        let state = PageState {
            tab: self.builds.tab.clone(),
            account: if self.login_active {
                "Cancel sign-in"
            } else if self.builds.account["authenticated"] == true {
                "Sign out"
            } else {
                "Sign in"
            }
            .into(),
            busy: self.build_busy(),
            planning: self.pending.contains_key("build:plan"),
            searching: self
                .pending
                .keys()
                .any(|k| k.starts_with("build:search") || k == "build:list"),
            added: !self.builds.added_group.is_empty(),
            error: self.error.clone(),
            notice: self.builds.notice.clone(),
        };
        let view = &mut self.build_view;
        view.sync(&self.ui, &mut self.builds, state, Instant::now());
        if !view.installed {
            self.ui.install_view(view.view(&self.ui));
            view.installed = true;
        }
        self.ui.update_view(FrameTime::now());
        view.bind(&mut self.ui, &self.builds);
    }
}
