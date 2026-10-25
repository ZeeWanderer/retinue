use super::artwork::Artwork;
use super::model::{Item, Model, Profile, number};
use retinue_ui::{
    Rect,
    composition::{Canvas, Element, Key, Recording, RecordingMode, Scene, Surface},
    damage::Damage,
    fontdue::{Font, FontSettings},
    line_edit::{Colors, LineEdit},
    painter::{CachedFont, GlyphCache, Painter, TextLine, css_rgba as rgba, text_width, wrap_text},
    taffy::{self, prelude::*},
};
use std::{cell::RefCell, rc::Rc};

mod plaques;
mod prices;
mod results;
#[cfg(test)]
mod scheduling_benchmark;

const MAIN: f32 = 424.0;
const CONTENT: f32 = 400.0;
const RAIL: f32 = 132.0;
const ITEM_TEXT_SIZE: f32 = 14.4;
const ITEM_SEPARATOR: u32 = 10;
const ITEM_LABEL: [u8; 4] = rgba(127, 127, 127, 255);
const ITEM_VALUE: [u8; 4] = rgba(255, 255, 255, 255);
const INK: [u8; 4] = rgba(230, 230, 223, 255);
const MUTED: [u8; 4] = rgba(159, 157, 138, 255);
const BORDER: [u8; 4] = rgba(71, 69, 56, 255);
const ACCENT: [u8; 4] = rgba(255, 227, 199, 255);
const PANEL: [u8; 4] = rgba(17, 17, 14, 255);
const BLUE: [u8; 4] = rgba(136, 136, 255, 255);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Close,
    Drag,
    League,
    LeagueOption(usize),
    Menu(Menu),
    Option(Menu, usize),
    Popup,
    DismissMenu,
    Level,
    Condition(usize),
    LevelMinimum,
    LevelMaximum,
    Sort,
    Base,
    Profile(Profile),
    Clear,
    Modifier(usize),
    ReadOnlyModifier(usize),
    Include(usize),
    Minimum(usize),
    Maximum(usize),
    Search,
    Trade,
    TradeLink,
    Listing(usize),
    Results,
    ItemScroll,
    EstimateInfo,
    EstimateAmount,
    RateBudget,
    ListingCurrency(usize),
    ScaleDown,
    ScaleUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Settings,
    Rarity,
    Status,
    Currency,
    Indexed,
}

impl Menu {
    pub fn key(self) -> &'static str {
        match self {
            Self::Settings => "",
            Self::Rarity => "rarity",
            Self::Status => "status",
            Self::Currency => "currency",
            Self::Indexed => "indexed",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Hit {
    pub target: Target,
    pub rect: Rect,
}

pub struct Frame {
    pub size: (u32, u32),
    pub scale: f32,
    pub zoom: f32,
    pub phase: f32,
}

#[derive(Default)]
pub struct Interaction {
    pub hover: Option<Target>,
    pub focus: Option<Target>,
    pub selecting: bool,
    pub tooltip: bool,
    pub league_open: bool,
    pub menu: Option<Menu>,
    pub menu_anchor: Option<Rect>,
    pub menu_scroll: usize,
    pub scroll: usize,
    pub item_scroll: usize,
    pub position: Option<(u32, u32)>,
    pub drag: Option<(f64, f64, u32, u32)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Node {
    Root,
    Main,
    Bar,
    Content,
    Card,
    Header,
    Item,
    Row(usize),
    Profiles,
    Estimate,
    Error,
    Search,
    Status,
    ResultFilters,
    Table,
    Listings,
    Footer,
    RateBudget,
}

impl From<Node> for Key {
    fn from(node: Node) -> Self {
        if let Node::Row(index) = node {
            return Self::new("evaluate.layout.row", index as u64);
        }
        let id = match node {
            Node::Root => 0,
            Node::Main => 1,
            Node::Bar => 2,
            Node::Content => 3,
            Node::Card => 4,
            Node::Header => 5,
            Node::Item => 6,
            Node::Profiles => 7,
            Node::Estimate => 8,
            Node::Error => 9,
            Node::Search => 10,
            Node::Status => 11,
            Node::ResultFilters => 12,
            Node::Table => 13,
            Node::Listings => 14,
            Node::Footer => 15,
            Node::RateBudget => 16,
            Node::Row(_) => unreachable!(),
        };
        Self::new("evaluate.layout", id)
    }
}

struct Layout(Vec<(Key, Rect)>);

impl Layout {
    fn find_bounds(&self, node: Node) -> Option<Rect> {
        let key = Key::from(node);
        self.0
            .iter()
            .find_map(|(id, rect)| (*id == key).then_some(*rect))
    }

    fn bounds(&self, node: Node) -> Rect {
        self.find_bounds(node).expect("Evaluate layout node")
    }
}

impl Interaction {
    pub fn close_popup(&mut self) -> bool {
        let open = self.menu.is_some() || self.league_open;
        self.menu = None;
        self.menu_anchor = None;
        self.menu_scroll = 0;
        self.league_open = false;
        open
    }

    pub fn focus_field(&mut self, target: Target, model: &Model) -> bool {
        if matches!(target, Target::LevelMinimum | Target::LevelMaximum) {
            if model.level.enabled {
                self.focus = Some(target);
                return true;
            }
            return false;
        }
        let index = match target {
            Target::Minimum(index) | Target::Maximum(index) => index,
            _ => return false,
        };
        if !model
            .filters
            .get(index)
            .is_some_and(|filter| filter.enabled)
        {
            return false;
        }
        self.focus = Some(target);
        true
    }

    pub fn leave_panel(&mut self) {
        self.hover = None;
        self.focus = None;
        self.selecting = false;
        self.tooltip = false;
    }
}

pub struct View {
    pub currencies: std::collections::BTreeMap<String, super::currency::Icon>,
    ui: Rc<CachedFont>,
    ui_bold: Rc<CachedFont>,
    ui_semibold: Rc<CachedFont>,
    item: Rc<CachedFont>,
    composition: Surface,
    layout_scene: RefCell<Scene>,
    editor_bounds: Option<Rect>,
    #[cfg(test)]
    glyphs: GlyphCache,
    artwork: Artwork,
    pub hits: Vec<Hit>,
    pub bounds: Rect,
    pub panel: Rect,
    pub item_rows: usize,
    pub visible_listings: usize,
    pub visible_options: usize,
    pub editor: LineEdit,
    pub preview_time: Option<u64>,
    render_budget: std::num::NonZeroU32,
    #[cfg(test)]
    render_threads: u32,
    scale: f32,
    animation: Damage,
    budget_fill: (f32, bool),
    time: u64,
    age_deadline: Option<u64>,
}

impl View {
    pub fn pointer_regions(&self) -> Vec<Rect> {
        std::iter::once(self.panel)
            .chain(self.hits.iter().map(|hit| hit.rect))
            .collect()
    }
    fn shield_popup(&mut self) {
        self.hits
            .extend(self.pointer_regions().into_iter().map(|rect| Hit {
                target: Target::DismissMenu,
                rect,
            }));
    }
    pub fn new() -> Result<Self, String> {
        let glyphs = GlyphCache::new(2 * 1024 * 1024);
        Ok(Self {
            currencies: Default::default(),
            ui: Rc::new(CachedFont::from_bytes(
                include_bytes!("../../assets/Lato-Regular.ttf").as_slice(),
                FontSettings::default(),
                &glyphs,
            )?),
            ui_bold: Rc::new(CachedFont::from_bytes(
                include_bytes!("../../assets/Lato-Bold.ttf").as_slice(),
                FontSettings::default(),
                &glyphs,
            )?),
            ui_semibold: Rc::new(CachedFont::from_bytes(
                include_bytes!("../../assets/Lato-Semibold.ttf").as_slice(),
                FontSettings::default(),
                &glyphs,
            )?),
            item: Rc::new(CachedFont::from_bytes(
                include_bytes!(concat!(env!("OUT_DIR"), "/Fontin-SmallCaps.ttf")).as_slice(),
                FontSettings::default(),
                &glyphs,
            )?),
            composition: Surface::default(),
            layout_scene: RefCell::default(),
            editor_bounds: None,
            #[cfg(test)]
            glyphs,
            artwork: Artwork::new()?,
            render_budget: retinue_ui::painter::render_thread_budget(4),
            #[cfg(test)]
            render_threads: 0,
            hits: Vec::new(),
            panel: Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            bounds: Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            item_rows: 0,
            visible_listings: 0,
            visible_options: 0,
            editor: LineEdit::new(include_bytes!("../../assets/Lato-Regular.ttf"), "Lato"),
            preview_time: None,
            scale: 1.0,
            animation: Damage::None,
            budget_fill: (0.0, false),
            time: current_time(),
            age_deadline: None,
        })
    }

    pub fn hit(&self, x: f64, y: f64) -> Option<Target> {
        self.hits
            .iter()
            .rev()
            .find(|h| h.rect.contains(x, y))
            .map(|h| h.target)
    }

    pub fn field_rect(&self, target: Target) -> Option<Rect> {
        self.hits
            .iter()
            .find(|h| h.target == target)
            .map(|h| h.rect.inset(px(3, self.scale)))
    }

    pub fn target_damage(&self, target: Option<Target>) -> Damage {
        let Some(target) = target else {
            return Damage::None;
        };
        self.hits
            .iter()
            .filter(|hit| hit.target == target)
            .map(|hit| hit.rect)
            .reduce(Rect::union)
            .map(|rect| Damage::rect(rect.expanded(px(6, self.scale))))
            .unwrap_or(Damage::Full)
    }

    pub fn animation_damage(&self) -> Damage {
        self.animation
    }

    pub fn budget_damage(&self, fill: (f32, bool)) -> Damage {
        if fill == self.budget_fill {
            Damage::None
        } else {
            self.target_damage(Some(Target::RateBudget))
        }
    }

    pub fn time_damage(&mut self, now: u64) -> Damage {
        if self.preview_time.is_some() || now == self.time {
            return Damage::None;
        }
        let changed = now < self.time || self.age_deadline.is_some_and(|deadline| now >= deadline);
        self.time = now;
        if changed && self.age_deadline.is_some() {
            self.target_damage(Some(Target::Results))
        } else {
            Damage::None
        }
    }

    pub fn time_deadline(&self, now: std::time::Instant) -> Option<std::time::Instant> {
        if self.preview_time.is_some() {
            return None;
        }
        self.age_deadline.map(|deadline| {
            let target = std::time::UNIX_EPOCH + std::time::Duration::from_secs(deadline);
            now + target
                .duration_since(std::time::SystemTime::now())
                .unwrap_or_default()
        })
    }

    pub fn begin_edit(&mut self, target: Target, value: &str, select: bool) {
        self.editor.set_text(value, select);
        if let Some(rect) = self.field_rect(target) {
            self.editor.layout(rect, 12.0 * self.scale);
        }
    }

    pub fn edit_click(&mut self, target: Target, x: f64, shift: bool) {
        if let Some(rect) = self.field_rect(target) {
            self.editor.click(x as f32 - rect.x as f32, shift);
        }
    }

    pub fn edit_drag(&mut self, target: Target, x: f64) {
        if let Some(rect) = self.field_rect(target) {
            self.editor.drag(x as f32 - rect.x as f32);
        }
    }

    pub fn next_field(
        &self,
        current: Option<Target>,
        backwards: bool,
        model: &Model,
    ) -> Option<Target> {
        let fields: Vec<_> = self
            .hits
            .iter()
            .filter_map(|hit| match hit.target {
                Target::LevelMinimum | Target::LevelMaximum if model.level.enabled => {
                    Some(hit.target)
                }
                Target::Minimum(index) | Target::Maximum(index)
                    if model
                        .filters
                        .get(index)
                        .is_some_and(|filter| filter.enabled) =>
                {
                    Some(hit.target)
                }
                _ => None,
            })
            .collect();
        let index = fields.iter().position(|f| Some(*f) == current)?;
        let next = if backwards {
            index.checked_sub(1).unwrap_or(fields.len() - 1)
        } else {
            (index + 1) % fields.len()
        };
        fields.get(next).copied()
    }

    pub fn paint(
        &mut self,
        pixels: &mut [u8],
        render: Frame,
        model: &Model,
        interaction: &Interaction,
    ) -> Result<(), String> {
        let repair = Rect {
            x: 0,
            y: 0,
            width: render.size.0,
            height: render.size.1,
        };
        self.paint_region(pixels, render, model, interaction, repair)
    }

    pub fn paint_region(
        &mut self,
        pixels: &mut [u8],
        render: Frame,
        model: &Model,
        interaction: &Interaction,
        repair: Rect,
    ) -> Result<(), String> {
        let Frame {
            size,
            scale,
            zoom,
            phase,
        } = render;
        self.hits.clear();
        self.animation = Damage::None;
        self.age_deadline = None;
        self.editor_bounds = None;
        let s = scale
            .min(size.0 as f32 / (MAIN + RAIL + 24.0))
            .min(size.1 as f32 / 640.0);
        self.scale = s;
        let width = ((MAIN + RAIL) * s).round() as u32;
        let preferred = interaction
            .position
            .unwrap_or(((size.0.saturating_sub(width)) / 2, (32.0 * s) as u32));
        let origin = (
            preferred.0.min(size.0.saturating_sub(width)),
            preferred.1.min(size.1.saturating_sub((640.0 * s) as u32)),
        );
        let rows = self.rows(model, s);
        self.item_rows = rows.len();
        let two_titles = model
            .evaluation
            .as_ref()
            .is_some_and(|e| !e.item.name.is_empty() && e.item.name != e.item.base);
        let layout = self.layout(
            size,
            origin,
            s,
            &rows,
            interaction.item_scroll,
            two_titles,
            model,
        )?;
        self.prepare_artwork(&layout, model, interaction, s)?;
        let mut painter = Painter::with_threads(pixels, size.0, size.1, self.render_budget)
            .map_err(|e| e.to_string())?;
        #[cfg(test)]
        {
            self.render_threads = painter.thread_count();
        }
        painter.clip_to_rect(repair);
        painter.clear();
        let mut p = Recording::new(size);
        let main = layout.bounds(Node::Main);
        self.panel = main;
        self.bounds = layout.bounds(Node::Root);
        fill(&mut p, main, rgba(0, 0, 0, 217));
        let bar = layout.bounds(Node::Bar);
        p.section(Key::new("evaluate.title", 0), bar);
        fill(&mut p, bar, rgba(22, 21, 18, 255));
        self.hits.push(Hit {
            target: Target::Drag,
            rect: bar,
        });
        for (target, x, icon) in [
            (Target::Trade, bar.x, Icon::Market),
            (
                Target::Menu(Menu::Settings),
                bar.x + bar.width - px(57, s),
                Icon::Settings,
            ),
            (Target::Close, bar.x + bar.width - px(28, s), Icon::Close),
        ] {
            let rect = Rect {
                x,
                y: bar.y,
                width: px(28, s),
                height: bar.height,
            };
            self.icon_button(&mut p, rect, target, icon, interaction, s);
        }
        let title_area = Rect {
            x: bar.x + px(28, s),
            width: bar.width.saturating_sub(px(85, s)),
            ..bar
        };
        let title_size = 12.0 * s;
        let prefix = "PoE2 - Evaluate ";
        let prefix_width = text_width(&self.ui, prefix, title_size).ceil() as u32;
        let league_text = elide(
            &self.ui,
            &format!(
                "({})",
                if model.league.is_empty() {
                    "League"
                } else {
                    &model.league
                }
            ),
            title_size,
            title_area.width.saturating_sub(prefix_width + px(8, s)),
        );
        let league_width = text_width(&self.ui, &league_text, title_size).ceil() as u32;
        let title_x =
            title_area.x + title_area.width.saturating_sub(prefix_width + league_width) / 2;
        p.draw_text_vertically_centered(
            &self.ui,
            TextLine::new(title_x, title_area.y, title_area.height),
            title_size,
            prefix,
            MUTED,
        );
        let league = Rect {
            x: title_x + prefix_width,
            width: league_width + px(4, s),
            ..title_area
        };
        self.hits.push(Hit {
            target: Target::League,
            rect: league,
        });
        p.draw_text_vertically_centered(
            &self.ui,
            TextLine::new(league.x, league.y, league.height),
            title_size,
            &league_text,
            if interaction.hover == Some(Target::League) {
                INK
            } else {
                ACCENT
            },
        );
        fill(
            &mut p,
            Rect {
                x: bar.x + bar.width - px(29, s),
                y: bar.y + px(6, s),
                width: u(s),
                height: bar.height - px(12, s),
            },
            BORDER,
        );

        let card = layout.bounds(Node::Card);
        let header = layout.bounds(Node::Header);
        let body = Rect {
            y: header.y + header.height,
            height: card.height - header.height,
            ..card
        };
        surface(&mut p, body, px(4, s), rgba(35, 34, 29, 255), PANEL, u(s));
        fill(
            &mut p,
            Rect {
                height: px(4, s),
                ..body
            },
            rgba(35, 34, 29, 255),
        );
        fill(
            &mut p,
            Rect {
                x: body.x + u(s),
                width: body.width - 2 * u(s),
                height: px(4, s),
                ..body
            },
            PANEL,
        );
        let fallback = Item {
            name: "Item evaluation".into(),
            rarity: "Normal".into(),
            ..Item::default()
        };
        let item_data = model
            .evaluation
            .as_ref()
            .map(|e| &e.item)
            .unwrap_or(&fallback);
        let rarity = item_data.rarity.as_str();
        self.paint_item_header(&mut p, header, item_data, s);

        let item = layout.bounds(Node::Item);
        p.section(Key::new("evaluate.item", 0), item);
        if rows
            .iter()
            .any(|r| r.modifier.is_some() || r.query.is_some())
        {
            let rail = Rect {
                x: main.x + main.width,
                y: item.y,
                width: px(128, s),
                height: card.y + card.height - item.y,
            };
            fill(&mut p, rail, PANEL);
            frame(&mut p, rail, rgba(35, 34, 29, 255), 1);
        }
        self.hits.push(Hit {
            target: Target::ItemScroll,
            rect: Rect {
                width: self.bounds.x + self.bounds.width - item.x,
                ..item
            },
        });
        let mut shown = 0;
        for (index, row) in rows.iter().enumerate().skip(interaction.item_scroll) {
            let Some(mut r) = layout.find_bounds(Node::Row(index)) else {
                break;
            };
            shown += 1;
            if row.separator {
                self.artwork.divider(
                    &mut p,
                    Rect {
                        y: r.y + u(s),
                        height: px(8, s),
                        ..r
                    },
                    rarity,
                    s,
                );
                r.y += px(ITEM_SEPARATOR, s);
                r.height = r.height.saturating_sub(px(ITEM_SEPARATOR, s));
            }
            let mut selected = false;
            if let Some(query) = row.query {
                let rail = Rect {
                    x: main.x + main.width + px(6, s),
                    y: r.y + (r.height.saturating_sub(px(20, s))) / 2,
                    width: px(116, s),
                    height: px(20, s),
                };
                match query {
                    Target::Menu(Menu::Rarity) => {
                        self.choice_button(&mut p, rail, Menu::Rarity, model, interaction, s);
                        selected = model.rarity.is_some();
                    }
                    Target::Level => {
                        self.hits.push(Hit {
                            target: Target::Level,
                            rect: r,
                        });
                        selected = model.level.enabled || interaction.hover == Some(Target::Level);
                        for (offset, target, value, hint) in [
                            (0, Target::LevelMinimum, &model.level.min, "Min"),
                            (1, Target::LevelMaximum, &model.level.max, "Max"),
                        ] {
                            let field = Rect {
                                x: rail.x + px(offset * 59, s),
                                width: px(56, s),
                                ..rail
                            };
                            self.numeric_field(
                                &mut p,
                                field,
                                target,
                                value,
                                hint,
                                model.level.enabled,
                                interaction,
                                s,
                            );
                        }
                    }
                    Target::Condition(index) => {
                        if model
                            .evaluation
                            .as_ref()
                            .and_then(|e| e.item.conditions.get(index))
                            .is_some_and(|c| c.id.is_some())
                        {
                            self.hits.push(Hit {
                                target: query,
                                rect: r,
                            });
                            selected = model.conditions.get(index).copied().unwrap_or(false)
                                || interaction.hover == Some(query);
                        }
                    }
                    _ => {}
                }
            }
            if let Some(mod_index) = row.modifier {
                let modifier = &model.evaluation.as_ref().unwrap().item.modifiers[mod_index];
                let filter = &model.filters[mod_index];
                if modifier.supported {
                    self.hits.push(Hit {
                        target: Target::Modifier(mod_index),
                        rect: r,
                    });
                    selected =
                        interaction.hover == Some(Target::Modifier(mod_index)) || filter.enabled;
                    let field_y = r.y + (r.height.saturating_sub(px(20, s))) / 2;
                    if modifier.min.is_some() {
                        self.hits.push(Hit {
                            target: Target::Include(mod_index),
                            rect: Rect {
                                x: main.x + main.width + px(6, s),
                                y: field_y,
                                width: px(115, s),
                                height: px(20, s),
                            },
                        });
                    }
                    for (offset, target, value, hint) in [
                        (0, Target::Minimum(mod_index), filter.min.as_str(), "Min"),
                        (1, Target::Maximum(mod_index), filter.max.as_str(), "Max"),
                    ]
                    .into_iter()
                    .filter(|_| modifier.min.is_some())
                    {
                        let field = Rect {
                            x: main.x + main.width + px(6 + offset * 59, s),
                            y: field_y,
                            width: px(56, s),
                            height: px(20, s),
                        };
                        self.numeric_field(
                            &mut p,
                            field,
                            target,
                            value,
                            hint,
                            filter.enabled,
                            interaction,
                            s,
                        );
                    }
                } else {
                    self.hits.push(Hit {
                        target: Target::ReadOnlyModifier(mod_index),
                        rect: r,
                    });
                }
            }
            self.paint_item_row(&mut p, r, row, rarity, selected, s);
        }
        if interaction.item_scroll > 0 || shown < rows.len() {
            let track = Rect {
                x: item.x + item.width - px(3, s),
                y: item.y,
                width: px(2, s),
                height: item.height,
            };
            fill(&mut p, track, BORDER);
            let height = track.height * shown.max(1) as u32 / rows.len().max(1) as u32;
            fill(
                &mut p,
                Rect {
                    y: track.y
                        + (track.height - height) * interaction.item_scroll as u32
                            / rows.len().saturating_sub(shown).max(1) as u32,
                    height,
                    ..track
                },
                MUTED,
            );
        }
        let profiles = layout.bounds(Node::Profiles);
        p.section(Key::new("evaluate.profiles", 0), profiles);
        self.artwork.divider(
            &mut p,
            Rect {
                y: profiles.y + u(s),
                height: px(8, s),
                ..profiles
            },
            rarity,
            s,
        );
        let segments = &item_data.profiles;
        let widths: Vec<_> = segments
            .iter()
            .map(|profile| {
                text_width(&self.ui, profile.label(), 14.0 * s).ceil() as u32 + px(24, s)
            })
            .collect();
        // Each reference label has a -11px leading and 24px trailing margin.
        let mut x = (profiles.x
            + profiles
                .width
                .saturating_sub(widths.iter().sum::<u32>() + px(13 * segments.len() as u32, s))
                / 2)
        .saturating_sub(px(11, s));
        for (&profile, width) in segments.iter().zip(widths) {
            let target = Target::Profile(profile);
            let label = profile.label();
            let rect = Rect {
                x,
                y: profiles.y + px(ITEM_SEPARATOR, s),
                height: px(24, s),
                width,
                ..profiles
            };
            x += width + px(13, s);
            self.hits.push(Hit { target, rect });
            let selected = model.profile == Some(profile);
            self.artwork.profile_radio(
                &mut p,
                Rect {
                    x: rect.x + px(4, s),
                    y: rect.y + px(4, s),
                    width: px(16, s),
                    height: px(16, s),
                },
                selected,
            );
            p.draw_text_vertically_centered(
                &self.ui,
                TextLine::new(rect.x + px(24, s), rect.y, rect.height),
                14.0 * s,
                label,
                MUTED,
            );
        }
        for x in [body.x, body.x + body.width - u(s)] {
            fill(
                &mut p,
                Rect {
                    x,
                    width: u(s),
                    height: body.height - px(4, s),
                    ..body
                },
                rgba(35, 34, 29, 255),
            );
        }
        if let Some(rect) = layout.find_bounds(Node::Estimate) {
            p.section(Key::new("evaluate.estimate", 0), rect);
            self.paint_estimate(&mut p, rect, model, s);
        }
        p.section(Key::new("evaluate.search", 0), layout.bounds(Node::Content));
        self.paint_search(&mut p, &layout, model, interaction, s, phase);
        p.section(Key::new("evaluate.popups", 0), self.bounds);
        if interaction.tooltip && interaction.menu.is_none() && !interaction.league_open {
            self.paint_listing_preview(&mut p, &layout, model, interaction, size, s);
        }
        self.paint_menu(&mut p, model, interaction, bar, size, s, zoom);
        if interaction.league_open {
            self.shield_popup();
            if let Some(evaluation) = &model.evaluation {
                for (i, l) in evaluation.leagues.iter().enumerate() {
                    let rect = Rect {
                        y: league.y + league.height + i as u32 * px(28, s),
                        height: px(28, s),
                        ..title_area
                    };
                    if rect.y + rect.height > size.1 {
                        break;
                    }
                    self.button(
                        &mut p,
                        rect,
                        Target::LeagueOption(i),
                        &l.text,
                        interaction,
                        s,
                    );
                }
            }
        }
        if interaction.tooltip {
            if let Some(target) = interaction.hover {
                let hint = match target {
                    Target::ScaleDown => Some("Zoom out"),
                    Target::ScaleUp => Some("Zoom in"),
                    Target::Close => Some("Close (Escape)"),
                    Target::Trade | Target::TradeLink => Some("Open trade search in browser"),
                    Target::ReadOnlyModifier(index) => {
                        Some(match item_data.modifiers[index].resolution.as_deref() {
                            Some("ambiguous") => "This modifier has conflicting trade mappings",
                            Some("unsupported_filter") => {
                                "This modifier needs an unsupported filter type"
                            }
                            _ => "No matching trade filter for this modifier",
                        })
                    }
                    Target::EstimateInfo => Some(
                        "Based on recent similar trades. Higher reliability with more consistent market data.",
                    ),
                    Target::EstimateAmount => {
                        Some("Estimated asking price; not a guaranteed sale price")
                    }
                    Target::RateBudget => Some(if self.budget_fill.1 {
                        "Trade requests are cooling down. Wait before searching again."
                    } else {
                        "Trade request usage. A full bar requires a short cooldown."
                    }),
                    Target::ListingCurrency(index) => model
                        .results
                        .as_ref()
                        .and_then(|r| r.listings.get(index))
                        .map(|row| {
                            self.currencies
                                .get(&row.currency)
                                .map_or(row.currency.as_str(), |icon| icon.source.text.as_str())
                        }),
                    Target::Menu(Menu::Settings) => Some("Settings"),
                    Target::Profile(Profile::Base) => {
                        Some("Match base, item level and fixed modifiers")
                    }
                    Target::Profile(Profile::Quick) => Some(
                        if item_data.ranking_status.as_deref() == Some("unavailable") {
                            "Ranking unavailable; select modifiers manually"
                        } else {
                            "Include the highest-ranked modifier groups"
                        },
                    ),
                    Target::Profile(Profile::Exact) => Some("Include all supported modifiers"),
                    Target::Profile(Profile::Broad) => {
                        Some("Reduce selected minimum bounds by 10%")
                    }
                    _ => None,
                };
                if let (Some(hint), Some(hit)) =
                    (hint, self.hits.iter().find(|h| h.target == target))
                {
                    let width = (text_width(&self.ui, hint, 12.0 * s).ceil() as u32 + px(16, s))
                        .min(self.bounds.width);
                    let rect = Rect {
                        x: hit.rect.x.min(self.bounds.x + self.bounds.width - width),
                        y: hit.rect.y + hit.rect.height + px(3, s),
                        width,
                        height: px(26, s),
                    };
                    fill(&mut p, rect, PANEL);
                    frame(&mut p, rect, BORDER, 1);
                    self.label(&mut p, rect, hint, 12.0 * s, INK, false);
                }
            }
        }
        let editor_bounds = self.editor_bounds;
        let editor = &mut self.editor;
        self.composition.draw(p, &mut painter, repair, |key, p| {
            if key == Key::new("evaluate.editor", 0) {
                if let Some(bounds) = editor_bounds {
                    editor.paint(
                        p,
                        Rect {
                            x: 0,
                            y: 0,
                            ..bounds
                        },
                        12.0 * s,
                        Colors {
                            text: INK,
                            selection: rgba(70, 63, 43, 255),
                        },
                    );
                }
            }
        })?;
        painter.finish().map_err(|e| e.to_string())
    }

    fn paint_search(
        &mut self,
        p: &mut Canvas,
        layout: &Layout,
        model: &Model,
        interaction: &Interaction,
        s: f32,
        phase: f32,
    ) {
        let cooldown = model.retry_after();
        self.budget_fill = model.budget.fill(std::time::Instant::now());
        if let Some(rect) = layout.find_bounds(Node::RateBudget) {
            p.fill_rounded_rect(
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                px(3, s),
                rgba(11, 11, 11, 255),
            );
            let width = (rect.width as f32 * self.budget_fill.0).round() as u32;
            if width > 0 {
                let x = if self.budget_fill.1 {
                    rect.x + rect.width - width
                } else {
                    rect.x
                };
                p.fill_rounded_rect(
                    x,
                    rect.y,
                    width,
                    rect.height,
                    px(3, s),
                    rgba(118, 109, 78, 255),
                );
            }
            self.hits.push(Hit {
                target: Target::RateBudget,
                rect,
            });
        }
        let unavailable = model
            .evaluation
            .as_ref()
            .is_some_and(|e| e.item.trade_unavailable.is_some());
        if let Some(search) = layout.find_bounds(Node::Search) {
            let hovered =
                !unavailable && cooldown == 0 && interaction.hover == Some(Target::Search);
            let highlighted = hovered || model.loading;
            let background = if highlighted {
                rgba(35, 34, 29, 255)
            } else {
                rgba(22, 21, 18, 255)
            };
            let disabled = unavailable || cooldown > 0;
            let border = if model.loading {
                ACCENT
            } else if disabled {
                rgba(50, 49, 46, 255)
            } else {
                rgba(118, 109, 78, 255)
            };
            let mut inner_border = if model.loading {
                ACCENT
            } else {
                rgba(118, 109, 78, 255)
            };
            inner_border[3] = 102;
            for (rect, radius, color) in [
                (search, 4, border),
                (search.inset(u(s)), 3, background),
                (search.inset(px(5, s)), 2, inner_border),
                (search.inset(px(6, s)), 1, background),
            ] {
                p.fill_rounded_rect(
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    px(radius, s),
                    color,
                );
            }
            let ornaments = search_ornament_rects(
                search,
                s,
                if model.loading {
                    6
                } else if hovered {
                    14
                } else {
                    10
                },
            );
            self.artwork.search_ornaments(p, ornaments, highlighted);
            if !model.loading && !unavailable && cooldown == 0 {
                self.hits
                    .extend(std::iter::once(search).chain(ornaments).map(|rect| Hit {
                        target: Target::Search,
                        rect,
                    }));
            }
            let label = if model.loading {
                if model.evaluation.is_some() {
                    "Searching...".into()
                } else {
                    "Reading item...".into()
                }
            } else if model.evaluation.is_none() {
                "Retry".into()
            } else if cooldown > 0 {
                format!("Search {cooldown}s")
            } else {
                "Search".into()
            };
            let text_width = text_width(&self.ui_bold, &label, 14.0 * s).ceil() as u32;
            p.draw_text_vertically_centered(
                &self.ui_bold,
                TextLine::new(
                    search.x + search.width.saturating_sub(text_width) / 2,
                    search.y,
                    search.height.saturating_sub(u(s)),
                ),
                14.0 * s,
                &label,
                if disabled {
                    rgba(255, 255, 255, 77)
                } else {
                    ACCENT
                },
            );
            if model.loading {
                self.animation = Damage::rect(search);
                fill(
                    p,
                    Rect {
                        x: search.x
                            + ((search.width.saturating_sub(px(64, s))) as f32 * phase) as u32,
                        y: search.y + search.height - u(s),
                        width: px(64, s),
                        height: u(s),
                    },
                    ACCENT,
                );
            }
        }
        if let Some(results) = &model.results {
            let status = layout.bounds(Node::Status);
            let inner = Rect {
                x: status.x + px(16, s),
                width: status.width.saturating_sub(px(32, s)),
                ..status
            };
            p.draw_text_vertically_centered(
                &self.ui,
                TextLine::new(inner.x, inner.y, inner.height),
                12.0 * s,
                &format!("{}/{}", results.listings.len(), results.returned),
                rgba(115, 113, 94, 255),
            );
            if model.loading || model.fetching || model.fetch_error.is_some() {
                let message = if model.loading {
                    "Searching...".to_owned()
                } else if model.fetching {
                    "Fetching...".to_owned()
                } else if cooldown > 0 {
                    format!("Unable to fetch data ({cooldown}s)")
                } else {
                    "Unable to fetch data".to_owned()
                };
                self.label(
                    p,
                    inner,
                    &message,
                    12.0 * s,
                    if model.fetch_error.is_some() {
                        rgba(232, 13, 13, 255)
                    } else {
                        rgba(115, 113, 94, 255)
                    },
                    false,
                );
            } else {
                let prefix = "Results from ";
                let url = "pathofexile.com/trade";
                let prefix_width = text_width(&self.ui, prefix, 12.0 * s).round() as u32;
                let link_width = text_width(&self.ui, url, 12.0 * s).round() as u32;
                let start = inner.x + inner.width.saturating_sub(prefix_width + link_width) / 2;
                let link = Rect {
                    x: start + prefix_width,
                    y: status.y,
                    width: link_width,
                    height: status.height,
                };
                self.hits.push(Hit {
                    target: Target::TradeLink,
                    rect: link,
                });
                p.draw_text_vertically_centered(
                    &self.ui,
                    TextLine::new(start, status.y, status.height),
                    12.0 * s,
                    prefix,
                    rgba(115, 113, 94, 255),
                );
                let color = if interaction.hover == Some(Target::TradeLink) {
                    INK
                } else {
                    ACCENT
                };
                p.draw_text_vertically_centered(
                    &self.ui,
                    TextLine::new(link.x, link.y, link.height),
                    12.0 * s,
                    url,
                    color,
                );
                let underline = (status.y + status.height / 2 + px(6, s)) as f64;
                p.draw_line(
                    (link.x as f64, underline),
                    ((link.x + link.width) as f64, underline),
                    s as f64,
                    color,
                );
            }
            let refresh = Rect {
                x: inner.x + inner.width - px(16, s),
                y: status.y + status.height.saturating_sub(px(16, s)) / 2,
                width: px(16, s),
                height: px(16, s),
            };
            if cooldown == 0 && !model.fetching && !model.loading {
                self.icon_button(p, refresh, Target::Search, Icon::Refresh, interaction, s);
            } else {
                self.artwork.icon(p, refresh, "refresh", false);
            }
            let filters = layout.bounds(Node::ResultFilters);
            for (i, menu) in [Menu::Currency, Menu::Status, Menu::Indexed]
                .into_iter()
                .enumerate()
            {
                let rect = Rect {
                    x: filters.x + px(i as u32 * 136, s),
                    width: px(128, s),
                    height: px(32, s),
                    ..filters
                };
                self.choice_button(p, rect, menu, model, interaction, s);
            }
            self.paint_results(p, layout, model, interaction, s);
        }
        if let Some(rect) = layout.find_bounds(Node::Error) {
            self.paint_error(p, rect, model.error.as_deref().unwrap_or_default(), s);
        }
        if let Some(footer) = layout.find_bounds(Node::Footer) {
            let message = "Not listed in the trade catalog";
            for (i, line) in wrap(&self.ui, message, 12.0 * s, footer.width - px(8, s))
                .iter()
                .take(2)
                .enumerate()
            {
                self.label(
                    p,
                    Rect {
                        y: footer.y + i as u32 * px(18, s),
                        height: px(18, s),
                        ..footer
                    },
                    line,
                    12.0 * s,
                    MUTED,
                    false,
                );
            }
        }
    }

    fn choice_button(
        &mut self,
        p: &mut Canvas,
        rect: Rect,
        menu: Menu,
        model: &Model,
        interaction: &Interaction,
        s: f32,
    ) {
        self.hits.push(Hit {
            target: Target::Menu(menu),
            rect,
        });
        surface(
            p,
            rect,
            px(4, s),
            BORDER,
            if interaction.hover == Some(Target::Menu(menu)) {
                rgba(37, 36, 30, 255)
            } else {
                rgba(22, 21, 18, 255)
            },
            u(s),
        );
        self.label(
            p,
            Rect {
                width: rect.width.saturating_sub(px(18, s)),
                ..rect
            },
            model.choice_label(menu.key()),
            if rect.height >= px(28, s) {
                14.0 * s
            } else {
                12.0 * s
            },
            MUTED,
            true,
        );
        let x = (rect.x + rect.width - px(10, s)) as f64;
        let y = (rect.y + rect.height / 2) as f64;
        p.draw_line(
            (x - 3.0 * s as f64, y - 1.0),
            (x, y + 2.0 * s as f64),
            s as f64,
            MUTED,
        );
        p.draw_line(
            (x, y + 2.0 * s as f64),
            (x + 3.0 * s as f64, y - 1.0),
            s as f64,
            MUTED,
        );
    }

    fn paint_menu(
        &mut self,
        p: &mut Canvas,
        model: &Model,
        interaction: &Interaction,
        bar: Rect,
        size: (u32, u32),
        s: f32,
        zoom: f32,
    ) {
        let Some(menu) = interaction.menu else { return };
        let anchor = self
            .hits
            .iter()
            .find(|h| h.target == Target::Menu(menu))
            .map(|h| h.rect)
            .or(interaction.menu_anchor)
            .unwrap_or(bar);
        let entries = model
            .evaluation
            .as_ref()
            .and_then(|e| e.search_options.get(menu.key()));
        let width = if menu == Menu::Settings {
            px(280, s)
        } else {
            entries
                .into_iter()
                .flatten()
                .map(|o| text_width(&self.ui, &o.text, 12.0 * s).ceil() as u32 + px(28, s))
                .max()
                .unwrap_or(0)
                .max(anchor.width)
                .min(px(360, s))
        };
        let rows = if menu == Menu::Settings {
            6
        } else {
            entries.map(Vec::len).unwrap_or(0)
        };
        let height = px(rows as u32 * 30 + 8, s).min(size.1);
        let rect = Rect {
            x: anchor.x.min(size.0.saturating_sub(width)),
            y: (anchor.y + anchor.height).min(size.1.saturating_sub(height)),
            width,
            height,
        };
        fill(p, rect, PANEL);
        frame(p, rect, BORDER, u(s));
        self.shield_popup();
        self.hits.push(Hit {
            target: Target::Popup,
            rect,
        });
        let x = self.bounds.x.min(rect.x);
        let y = self.bounds.y.min(rect.y);
        self.bounds = Rect {
            x,
            y,
            width: (rect.x + rect.width).max(self.bounds.x + self.bounds.width) - x,
            height: (rect.y + rect.height).max(self.bounds.y + self.bounds.height) - y,
        };
        if menu == Menu::Settings {
            let row = Rect {
                x: rect.x + px(6, s),
                y: rect.y + px(4, s),
                width: rect.width - px(12, s),
                height: px(30, s),
            };
            self.label(p, cell(row, 0.0, 0.45), "UI scale", 12.0 * s, INK, true);
            self.icon_button(
                p,
                cell(row, 0.48, 0.62),
                Target::ScaleDown,
                Icon::Minus,
                interaction,
                s,
            );
            self.label(
                p,
                cell(row, 0.62, 0.84),
                &format!("{}%", (zoom * 100.0).round()),
                12.0 * s,
                INK,
                false,
            );
            self.icon_button(
                p,
                cell(row, 0.84, 1.0),
                Target::ScaleUp,
                Icon::Plus,
                interaction,
                s,
            );
            for (i, sub) in [Menu::Status, Menu::Currency, Menu::Indexed]
                .into_iter()
                .enumerate()
            {
                self.choice_button(
                    p,
                    Rect {
                        y: row.y + px((i as u32 + 1) * 30, s),
                        ..row
                    },
                    sub,
                    model,
                    interaction,
                    s,
                );
            }
            self.button(
                p,
                Rect {
                    y: row.y + px(120, s),
                    ..row
                },
                Target::Base,
                if model.base {
                    "Match item base: on"
                } else {
                    "Match item base: off"
                },
                interaction,
                s,
            );
            self.button(
                p,
                Rect {
                    y: row.y + px(150, s),
                    ..row
                },
                Target::Clear,
                "Clear selected modifiers",
                interaction,
                s,
            );
        } else if let Some(entries) = entries {
            self.visible_options = ((rect.height.saturating_sub(px(8, s)) as f32 / (30.0 * s))
                + 0.01)
                .floor()
                .max(1.0) as usize;
            let start = interaction
                .menu_scroll
                .min(entries.len().saturating_sub(self.visible_options));
            for (offset, (i, option)) in entries
                .iter()
                .enumerate()
                .skip(start)
                .take(self.visible_options)
                .enumerate()
            {
                let row = Rect {
                    x: rect.x + px(4, s),
                    y: rect.y + px(4 + offset as u32 * 30, s),
                    width: rect.width - px(8, s),
                    height: px(4 + (offset as u32 + 1) * 30, s) - px(4 + offset as u32 * 30, s),
                };
                let selected = option.text == model.choice_label(menu.key());
                let target = Target::Option(menu, i);
                self.hits.push(Hit { target, rect: row });
                if selected || interaction.hover == Some(target) {
                    fill(p, row, rgba(37, 36, 30, 255));
                }
                self.label(
                    p,
                    row,
                    &option.text,
                    12.0 * s,
                    if selected { INK } else { MUTED },
                    true,
                );
                if selected {
                    fill(
                        p,
                        Rect {
                            width: px(2, s),
                            ..row
                        },
                        ACCENT,
                    );
                }
            }
        }
    }

    fn numeric_field(
        &mut self,
        p: &mut Canvas,
        field: Rect,
        target: Target,
        value: &str,
        hint: &str,
        enabled: bool,
        interaction: &Interaction,
        s: f32,
    ) {
        self.hits.push(Hit {
            target,
            rect: field,
        });
        surface(
            p,
            field,
            px(4, s),
            if interaction.focus == Some(target) {
                ACCENT
            } else {
                BORDER
            },
            if enabled {
                rgba(22, 21, 18, 255)
            } else {
                rgba(45, 44, 40, 255)
            },
            u(s),
        );
        if interaction.focus == Some(target) {
            let bounds = field.inset(px(3, s));
            self.editor_bounds = Some(bounds);
            p.external(Key::new("evaluate.editor", 0), bounds);
        } else {
            self.label(
                p,
                field,
                if value.is_empty() { hint } else { value },
                12.0 * s,
                if enabled { INK } else { MUTED },
                !value.is_empty(),
            );
        }
    }

    fn rows(&self, model: &Model, s: f32) -> Vec<Row> {
        let Some(e) = &model.evaluation else {
            return Vec::new();
        };
        self.rows_for_item(&e.item, s)
    }

    fn rows_for_item(&self, item: &Item, s: f32) -> Vec<Row> {
        let mut rows = Vec::new();
        if matches!(item.rarity.as_str(), "Normal" | "Magic" | "Rare") {
            rows.push(Row {
                lines: vec![format!("Item Rarity: {}", item.rarity)],
                modifier: None,
                separator: false,
                query: Some(Target::Menu(Menu::Rarity)),
            });
        }
        if let Some(level) = item.item_level {
            rows.push(Row {
                lines: vec![format!("Item Level: {level}")],
                modifier: None,
                separator: false,
                query: Some(Target::Level),
            });
        }
        for property in &item.properties {
            rows.push(Row {
                lines: wrap(
                    &self.item,
                    &property.text,
                    ITEM_TEXT_SIZE * s,
                    ((CONTENT - 32.0) * s) as u32,
                ),
                modifier: None,
                separator: false,
                query: None,
            });
        }
        for (i, m) in item.modifiers.iter().enumerate() {
            rows.push(Row {
                lines: wrap(
                    &self.item,
                    &m.text,
                    ITEM_TEXT_SIZE * s,
                    ((CONTENT - 32.0) * s) as u32,
                ),
                modifier: Some(i),
                separator: false,
                query: None,
            });
        }
        if item.rarity == "Gem" {
            rows.sort_by_key(|row| row.modifier.is_none());
        }
        for index in 1..rows.len() {
            let group = |row: &Row| {
                (
                    row.modifier.map(|i| item.modifiers[i].section.as_str()),
                    row.query.is_some(),
                )
            };
            rows[index].separator = group(&rows[index]) != group(&rows[index - 1]);
        }
        for (index, condition) in item.conditions.iter().enumerate() {
            rows.push(Row {
                lines: vec![condition.text.clone()],
                modifier: None,
                separator: index == 0,
                query: Some(Target::Condition(index)),
            });
        }
        rows
    }
    fn layout(
        &self,
        size: (u32, u32),
        origin: (u32, u32),
        s: f32,
        rows: &[Row],
        scroll: usize,
        two_titles: bool,
        model: &Model,
    ) -> Result<Layout, String> {
        let bar = Element::new(Node::Bar, fixed(MAIN * s, 28.0 * s));
        let header_height = if two_titles { 54.0 } else { 34.0 };
        let header = Element::new(Node::Header, fixed(CONTENT * s, header_height * s));
        let results = model.results.is_some();
        let error_height = model
            .error
            .as_ref()
            .map(|message| {
                36.0 + 17.0
                    * wrap(&self.ui, message, 14.0 * s, px(CONTENT as u32 - 52, s))
                        .len()
                        .max(1) as f32
            })
            .unwrap_or(0.0);
        let estimate_height = plaques::estimate_height(model);
        let extra = if results { 388.0 } else { 56.0 }
            + if model.budget.visible() { 11.0 } else { 0.0 }
            + if estimate_height > 0.0 {
                estimate_height + 8.0
            } else {
                0.0
            }
            + if error_height > 0.0 {
                error_height + 8.0
            } else {
                0.0
            };
        let max_item = (size.1 as f32 - origin.1 as f32 - (96.0 + extra + header_height) * s)
            .min(390.0 * s)
            .max(90.0 * s);
        let viewport = (rows.iter().map(|r| r.height() * s).sum::<f32>() + 6.0 * s)
            .min(max_item)
            .max(38.0 * s);
        let mut used = 0.0;
        let mut item_nodes = Vec::new();
        for (i, row) in rows.iter().enumerate().skip(scroll) {
            let h = row.height() * s;
            if used + h > viewport - 6.0 * s {
                break;
            }
            item_nodes.push(Element::new(Node::Row(i), fixed(CONTENT * s, h)));
            used += h;
        }
        let item = Element::new(
            Node::Item,
            Style {
                flex_direction: FlexDirection::Column,

                padding: taffy::Rect {
                    top: length(6.0 * s),
                    bottom: length(0.0_f32),
                    left: length(0.0_f32),
                    right: length(0.0_f32),
                },
                ..fixed(CONTENT * s, viewport)
            },
        )
        .with_children(item_nodes);
        let profiles = Element::new(Node::Profiles, fixed(CONTENT * s, 41.0 * s));
        let card = Element::new(
            Node::Card,
            Style {
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
        )
        .with_children(vec![header, item, profiles]);
        let mut nodes = vec![card];
        if estimate_height > 0.0 {
            let mut style = fixed(CONTENT * s, estimate_height * s);
            style.margin.top = length(8.0 * s);
            nodes.push(Element::new(Node::Estimate, style));
        }
        if error_height > 0.0 {
            let mut style = fixed(CONTENT * s, error_height * s);
            style.margin.top = length(8.0 * s);
            nodes.push(Element::new(Node::Error, style));
        }
        let mut sections = if results {
            vec![
                (Node::Status, 21.0),
                (Node::ResultFilters, 33.0),
                (Node::Table, 32.0),
                (Node::Listings, 268.0),
            ]
        } else {
            vec![(Node::Search, 40.0)]
        };
        if model.budget.visible() {
            sections.push((Node::RateBudget, 3.0));
        }
        if model
            .evaluation
            .as_ref()
            .is_some_and(|e| e.item.trade_unavailable.is_some())
        {
            sections.push((Node::Footer, 40.0));
        }
        for (key, h) in sections {
            let mut style = fixed(CONTENT * s, h * s);
            if matches!(
                key,
                Node::Search | Node::Status | Node::ResultFilters | Node::Table | Node::RateBudget
            ) {
                style.margin.top = length(8.0 * s);
            }
            nodes.push(Element::new(key, style));
        }
        let content = Element::new(
            Node::Content,
            Style {
                flex_direction: FlexDirection::Column,

                size: Size {
                    width: length(MAIN * s),
                    height: auto(),
                },
                padding: taffy::Rect {
                    left: length(12.0 * s),
                    right: length(12.0 * s),
                    top: length(12.0 * s),
                    bottom: length(12.0 * s),
                },
                ..Default::default()
            },
        )
        .with_children(nodes);
        let main = Element::new(
            Node::Main,
            Style {
                flex_direction: FlexDirection::Column,

                size: Size {
                    width: length(MAIN * s),
                    height: auto(),
                },
                ..Default::default()
            },
        )
        .with_children(vec![bar, content]);
        let mut root = Element::new(
            Node::Root,
            Style {
                size: Size {
                    width: length((MAIN + RAIL) * s),
                    height: auto(),
                },
                ..Default::default()
            },
        )
        .with_children(vec![main]);
        root.offset = (origin.0 as f32, origin.1 as f32);
        let mut scene = self.layout_scene.borrow_mut();
        scene.update(root, size, 1.0, RecordingMode::Retained)?;
        Ok(Layout(
            scene
                .geometry()
                .iter()
                .map(|node| (node.key, scene.layout_rect(node.key).unwrap()))
                .collect(),
        ))
    }
    fn button(
        &mut self,
        p: &mut Canvas,
        rect: Rect,
        target: Target,
        text: &str,
        i: &Interaction,
        s: f32,
    ) {
        self.hits.push(Hit { target, rect });
        let rect = rect.inset(px(2, s));
        p.fill_rounded_rect(rect.x, rect.y, rect.width, rect.height, px(2, s), BORDER);
        let inside = rect.inset(u(s));
        p.fill_rounded_rect(
            inside.x,
            inside.y,
            inside.width,
            inside.height,
            u(s),
            if i.hover == Some(target) {
                rgba(59, 55, 43, 255)
            } else {
                rgba(33, 31, 25, 255)
            },
        );
        self.label(p, rect, text, 12.0 * s, INK, false);
    }
    fn icon_button(
        &mut self,
        p: &mut Canvas,
        rect: Rect,
        target: Target,
        icon: Icon,
        i: &Interaction,
        s: f32,
    ) {
        self.hits.push(Hit { target, rect });
        if i.hover == Some(target) {
            fill(
                p,
                rect,
                match icon {
                    Icon::Close => rgba(239, 67, 67, 255),
                    Icon::Market | Icon::Settings => rgba(22, 21, 18, 255),
                    _ => rgba(59, 55, 43, 255),
                },
            );
        }
        let (x, y) = (
            rect.x as f64 + rect.width as f64 / 2.0,
            rect.y as f64 + rect.height as f64 / 2.0,
        );
        let r = 4.0 * s as f64;
        match icon {
            Icon::Close | Icon::Market | Icon::Settings | Icon::Refresh => {
                let name = match icon {
                    Icon::Close => "close",
                    Icon::Market => "market",
                    Icon::Refresh => "refresh",
                    _ => "settings",
                };
                let side = px(16, s);
                self.artwork.icon(
                    p,
                    Rect {
                        x: rect.x + (rect.width - side) / 2,
                        y: rect.y + (rect.height - side) / 2,
                        width: side,
                        height: side,
                    },
                    name,
                    i.hover == Some(target),
                );
            }
            Icon::Minus | Icon::Plus => {
                p.draw_line((x - r, y), (x + r, y), s as f64 * 1.5, INK);
                if matches!(icon, Icon::Plus) {
                    p.draw_line((x, y - r), (x, y + r), s as f64 * 1.5, INK);
                }
            }
        }
    }
    fn label(
        &self,
        p: &mut Canvas,
        rect: Rect,
        text: &str,
        size: f32,
        color: [u8; 4],
        leading: bool,
    ) {
        let padding = px(6, self.scale);
        let text = elide(&self.ui, text, size, rect.width.saturating_sub(padding * 2));
        let x = rect.x
            + if leading {
                padding
            } else {
                (rect
                    .width
                    .saturating_sub(text_width(&self.ui, &text, size) as u32))
                    / 2
            };
        p.draw_text_vertically_centered(
            &self.ui,
            TextLine::new(x, rect.y, rect.height),
            size,
            &text,
            color,
        );
    }
    fn item_label(&self, p: &mut Canvas, rect: Rect, text: &str, size: f32, color: [u8; 4]) {
        let text = elide(&self.item, text, size, rect.width);
        let width = text_width(&self.item, &text, size).ceil() as u32;
        p.draw_text_vertically_centered(
            &self.item,
            TextLine::new(
                rect.x + rect.width.saturating_sub(width) / 2,
                rect.y,
                rect.height,
            ),
            size,
            &text,
            color,
        );
    }
}

struct Row {
    lines: Vec<String>,
    modifier: Option<usize>,
    separator: bool,
    query: Option<Target>,
}
impl Row {
    fn height(&self) -> f32 {
        self.lines.len() as f32 * 20.0
            + if self.separator {
                ITEM_SEPARATOR as f32
            } else {
                0.0
            }
    }
}
enum Icon {
    Close,
    Minus,
    Plus,
    Market,
    Settings,
    Refresh,
}
fn u(s: f32) -> u32 {
    s.round().max(1.0) as u32
}
fn rarity_color(rarity: &str) -> [u8; 4] {
    match rarity.to_ascii_lowercase().as_str() {
        "rare" => rgba(255, 255, 119, 255),
        "unique" => rgba(175, 96, 37, 255),
        "magic" => BLUE,
        "gem" => rgba(27, 162, 155, 255),
        "currency" => rgba(170, 158, 130, 255),
        "quest" => rgba(74, 230, 58, 255),
        _ => rgba(200, 200, 200, 255),
    }
}

fn age(indexed: Option<u64>, now: u64) -> String {
    let Some(indexed) = indexed else {
        return "-".into();
    };
    let seconds = now.saturating_sub(indexed);
    let (value, unit) = if seconds >= 86400 {
        (seconds / 86400, "d")
    } else if seconds >= 3600 {
        (seconds / 3600, "h")
    } else if seconds >= 60 {
        (seconds / 60, "m")
    } else {
        (seconds, "s")
    };
    format!("{value}{unit} ago")
}

pub(super) fn current_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn px(value: u32, s: f32) -> u32 {
    (value as f32 * s).round() as u32
}
fn fixed(w: f32, h: f32) -> Style {
    Style {
        size: Size {
            width: length(w),
            height: length(h),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn cell(r: Rect, from: f32, to: f32) -> Rect {
    Rect {
        x: r.x + (r.width as f32 * from) as u32,
        width: (r.width as f32 * (to - from)) as u32,
        ..r
    }
}
fn fill(p: &mut Canvas, r: Rect, c: [u8; 4]) {
    p.fill_rounded_rect(r.x, r.y, r.width, r.height, 0, c);
}
fn surface(
    p: &mut Canvas,
    r: Rect,
    radius: u32,
    border: [u8; 4],
    background: [u8; 4],
    stroke: u32,
) {
    p.fill_rounded_rect(r.x, r.y, r.width, r.height, radius, border);
    let inner = r.inset(stroke);
    p.fill_rounded_rect(
        inner.x,
        inner.y,
        inner.width,
        inner.height,
        radius.saturating_sub(stroke),
        background,
    );
}
fn frame(p: &mut Canvas, r: Rect, c: [u8; 4], w: u32) {
    for edge in [
        Rect { height: w, ..r },
        Rect {
            y: r.y + r.height.saturating_sub(w),
            height: w,
            ..r
        },
        Rect { width: w, ..r },
        Rect {
            x: r.x + r.width.saturating_sub(w),
            width: w,
            ..r
        },
    ] {
        fill(p, edge, c);
    }
}
fn search_ornament_rects(button: Rect, scale: f32, offset: u32) -> [Rect; 2] {
    let width = px(24, scale);
    let height = px(20, scale);
    let protrusion = px(offset, scale).saturating_sub(u(scale));
    [
        button.x.saturating_sub(protrusion),
        (button.x + button.width + protrusion).saturating_sub(width),
    ]
    .map(|x| Rect {
        x,
        y: button.y + button.height.saturating_sub(height) / 2,
        width,
        height,
    })
}

fn elide(font: &Font, text: &str, size: f32, width: u32) -> String {
    if text_width(font, text, size) <= width as f32 {
        return text.into();
    }
    let mut result = text.to_owned();
    while !result.is_empty() && text_width(font, &format!("{result}..."), size) > width as f32 {
        result.pop();
    }
    format!("{result}...")
}
fn wrap(font: &Font, text: &str, size: f32, width: u32) -> Vec<String> {
    wrap_text(font, text, size, width as f32)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_same_pixels(actual: &[u8], expected: &[u8], width: u32, label: &str) {
        // Native clipping can round antialiased coverage by one channel level.
        if let Some(index) = actual
            .iter()
            .zip(expected)
            .position(|(a, b)| a.abs_diff(*b) > 1)
        {
            panic!(
                "{label}: mismatch at {},{}: {} != {}",
                (index / 4) % width as usize,
                (index / 4) / width as usize,
                actual[index],
                expected[index],
            );
        }
    }

    #[test]
    fn partial_hover_matches_full_paint_across_three_buffers() {
        use retinue_ui::damage::{Damage, RepaintTracker};
        for (size, scale) in [
            ((1920, 1080), 1.0),
            ((2560, 1440), 1.25),
            ((2560, 1440), 2.0),
        ] {
            let mut view = View::new().unwrap();
            let mut oracle = View::new().unwrap();
            let model = fixture();
            let mut interaction = Interaction::default();
            let mut tracker = RepaintTracker::default();
            let mut buffers: Vec<_> = (0..3)
                .map(|_| {
                    tracker.add_buffer();
                    vec![113; (size.0 * size.1 * 4) as usize]
                })
                .collect();
            let mut full = vec![0; buffers[0].len()];
            let frame = || Frame {
                size,
                scale,
                zoom: scale,
                phase: 0.0,
            };
            view.paint(&mut full, frame(), &model, &interaction)
                .unwrap();
            let mut targets = Vec::new();
            for hit in &view.hits {
                if !targets.contains(&Some(hit.target)) {
                    targets.push(Some(hit.target));
                }
            }
            targets.push(None);
            for (step, target) in targets.iter().cycle().take(targets.len() * 2).enumerate() {
                let old = interaction.hover;
                interaction.hover = *target;
                tracker.invalidate(view.target_damage(old).union(view.target_damage(*target)));
                let index = step % buffers.len();
                let plan = tracker.plan(index, size).unwrap();
                view.paint_region(
                    &mut buffers[index],
                    frame(),
                    &model,
                    &interaction,
                    plan.repair,
                )
                .unwrap();
                oracle
                    .paint(&mut full, frame(), &model, &interaction)
                    .unwrap();
                assert_same_pixels(
                    &buffers[index],
                    &full,
                    size.0,
                    &format!("{target:?}, scale={scale}, step={step}, {plan:?}"),
                );
                tracker.submitted(index);
            }
            // Removal of a translucent popup must also restore its old background.
            interaction.tooltip = true;
            interaction.hover = Some(Target::Close);
            tracker.invalidate(Damage::Full);
            let plan = tracker.plan(0, size).unwrap();
            view.paint_region(&mut buffers[0], frame(), &model, &interaction, plan.repair)
                .unwrap();
            tracker.submitted(0);
            interaction.tooltip = false;
            interaction.hover = None;
            tracker.invalidate(Damage::Full);
            let plan = tracker.plan(0, size).unwrap();
            view.paint_region(&mut buffers[0], frame(), &model, &interaction, plan.repair)
                .unwrap();
            oracle
                .paint(&mut full, frame(), &model, &interaction)
                .unwrap();
            assert_same_pixels(&buffers[0], &full, size.0, "tooltip removal");
        }
    }

    #[test]
    fn partial_loading_pulse_and_editor_match_full_paint() {
        for scale in [1.0, 1.25, 2.0] {
            let size = (2560, 1440);
            let mut view = View::new().unwrap();
            let mut oracle = View::new().unwrap();
            let mut model = fixture();
            model.loading = true;
            let mut interaction = Interaction::default();
            let mut partial = vec![0; (size.0 * size.1 * 4) as usize];
            let mut full = partial.clone();
            let frame = |phase| Frame {
                size,
                scale,
                zoom: scale,
                phase,
            };
            view.paint(&mut partial, frame(0.0), &model, &interaction)
                .unwrap();
            for phase in [0.1, 0.5, 1.0, 0.6, 0.0] {
                let repair = view.animation_damage().bounds(size).unwrap();
                view.paint_region(&mut partial, frame(phase), &model, &interaction, repair)
                    .unwrap();
                oracle
                    .paint(&mut full, frame(phase), &model, &interaction)
                    .unwrap();
                assert_same_pixels(&partial, &full, size.0, "loading pulse");
            }
            model.loading = false;
            model.level.enabled = true;
            interaction.focus = Some(Target::LevelMinimum);
            view.paint(&mut partial, frame(0.0), &model, &interaction)
                .unwrap();
            oracle
                .paint(&mut full, frame(0.0), &model, &interaction)
                .unwrap();
            for select in [true, false] {
                view.begin_edit(Target::LevelMinimum, "83", select);
                oracle.begin_edit(Target::LevelMinimum, "83", select);
                let repair = view.target_damage(interaction.focus).bounds(size).unwrap();
                view.paint_region(&mut partial, frame(0.0), &model, &interaction, repair)
                    .unwrap();
                oracle
                    .paint(&mut full, frame(0.0), &model, &interaction)
                    .unwrap();
                assert_same_pixels(&partial, &full, size.0, "editor selection");
            }
        }
    }

    #[test]
    fn listing_ages_invalidate_at_the_next_displayed_boundary() {
        use retinue_ui::damage::Damage;
        let data: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/evaluate-reference.json"
        ))
        .unwrap();
        let mut model = fixture();
        let mut results: super::super::model::Results =
            serde_json::from_value(data["results"].clone()).unwrap();
        for listing in &mut results.listings {
            listing.indexed_at = Some(1000);
        }
        model.results = Some(results);
        let mut view = View::new().unwrap();
        let mut oracle = View::new().unwrap();
        view.time = 1000;
        let size = (1920, 1080);
        let frame = || Frame {
            size,
            scale: 1.25,
            zoom: 1.25,
            phase: 0.0,
        };
        let interaction = Interaction::default();
        let mut partial = vec![0; (size.0 * size.1 * 4) as usize];
        let mut full = partial.clone();
        view.paint(&mut partial, frame(), &model, &interaction)
            .unwrap();
        assert_eq!(view.age_deadline, Some(1001));
        assert_eq!(view.time_damage(1000), Damage::None);
        for now in [1001, 1059, 1060] {
            let repair = view.time_damage(now).bounds(size).unwrap();
            view.paint_region(&mut partial, frame(), &model, &interaction, repair)
                .unwrap();
            oracle.time = now;
            oracle
                .paint(&mut full, frame(), &model, &interaction)
                .unwrap();
            assert_same_pixels(&partial, &full, size.0, "listing age");
        }
        assert_eq!(view.age_deadline, Some(1120));
        assert_eq!(view.time_damage(1061), Damage::None);
        assert_ne!(view.time_damage(1000), Damage::None);
        view.preview_time = Some(1000);
        assert_eq!(view.time_damage(1200), Damage::None);
    }

    #[test]
    fn item_border_is_continuous_and_value_and_error_plaques_coexist() {
        let mut view = View::new().unwrap();
        let mut model = fixture();
        model.estimate = Some(super::super::model::Estimate {
            amount: 40.0,
            currency: "exalted".into(),
            uncertainty: None,
        });
        model.error = Some("Trade service unavailable".into());
        for scale in [1.0, 1.25, 2.0] {
            let size = (2560, 1440);
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            let rows = view.rows(&model, scale);
            let layout = view
                .layout(size, (0, 0), scale, &rows, 0, true, &model)
                .unwrap();
            let item = layout.bounds(Node::Item);
            let card = layout.bounds(Node::Card);
            let estimate = layout.bounds(Node::Estimate);
            let error = layout.bounds(Node::Error);
            assert!(estimate.y.abs_diff(card.y + card.height + px(8, scale)) <= 1);
            assert!(
                error
                    .y
                    .abs_diff(estimate.y + estimate.height + px(8, scale))
                    <= 1
            );
            view.paint(
                &mut pixels,
                Frame {
                    size,
                    scale,
                    zoom: scale,
                    phase: 0.0,
                },
                &model,
                &Interaction {
                    position: Some((0, 0)),
                    ..Interaction::default()
                },
            )
            .unwrap();
            for x in [item.x, item.x + item.width - 1] {
                for y in item.y..item.y + item.height {
                    let index = (y * size.0 + x) as usize * 4;
                    assert_eq!(
                        &pixels[index..index + 4],
                        &rgba(35, 34, 29, 255),
                        "edge {x},{y} at {scale}"
                    );
                }
            }
        }
    }

    #[test]
    fn search_button_matches_reference_bounds_colors_and_ornament_hits() {
        let mut view = View::new().unwrap();
        let size = (2560, 1440);
        let model = fixture();
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        for scale in [1.0, 1.25, 2.0] {
            for hovered in [false, true] {
                let interaction = Interaction {
                    hover: hovered.then_some(Target::Search),
                    ..Interaction::default()
                };
                view.paint(
                    &mut pixels,
                    Frame {
                        size,
                        scale,
                        zoom: 1.0,
                        phase: 0.0,
                    },
                    &model,
                    &interaction,
                )
                .unwrap();
                let hits: Vec<_> = view
                    .hits
                    .iter()
                    .filter(|hit| hit.target == Target::Search)
                    .collect();
                assert_eq!(hits.len(), 3);
                let button = hits[0].rect;
                assert_eq!(
                    (button.width, button.height),
                    (px(400, scale), px(40, scale))
                );
                assert_eq!(button.x, view.panel.x + px(12, scale));
                let caps = search_ornament_rects(button, scale, if hovered { 14 } else { 10 });
                for (hit, cap) in hits[1..].iter().zip(caps) {
                    assert_eq!(hit.rect, cap);
                    assert_eq!((cap.width, cap.height), (px(24, scale), px(20, scale)));
                    assert_eq!(
                        view.hit(
                            (cap.x + cap.width / 2) as f64,
                            (cap.y + cap.height / 2) as f64
                        ),
                        Some(Target::Search)
                    );
                }
                let pixel = |x, y| {
                    &pixels[((y * size.0 + x) * 4) as usize..((y * size.0 + x) * 4 + 4) as usize]
                };
                assert_eq!(
                    pixel(button.x + button.width / 2, button.y),
                    rgba(118, 109, 78, 255)
                );
                assert_eq!(
                    pixel(button.x + px(40, scale), button.y + button.height / 2),
                    if hovered {
                        rgba(35, 34, 29, 255)
                    } else {
                        rgba(22, 21, 18, 255)
                    }
                );
            }
        }
        let mut loading = model;
        loading.loading = true;
        view.paint(
            &mut pixels,
            Frame {
                size,
                scale: 1.0,
                zoom: 1.0,
                phase: 0.0,
            },
            &loading,
            &Interaction::default(),
        )
        .unwrap();
        assert!(!view.hits.iter().any(|hit| hit.target == Target::Search));
    }

    #[test]
    fn title_bar_buttons_keep_reference_bounds_and_icon_color() {
        let mut view = View::new().unwrap();
        let size = (2560, 1440);
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        view.paint(
            &mut pixels,
            Frame {
                size,
                scale: 1.0,
                zoom: 1.0,
                phase: 0.0,
            },
            &fixture(),
            &Interaction::default(),
        )
        .unwrap();
        for (target, offset) in [
            (Target::Trade, 0),
            (Target::Menu(Menu::Settings), 367),
            (Target::Close, 396),
        ] {
            let rect = view
                .hits
                .iter()
                .find(|hit| hit.target == target)
                .unwrap()
                .rect;
            assert_eq!(rect.x, view.panel.x + offset);
            assert_eq!((rect.width, rect.height), (28, 28));
            let matches = (rect.y..rect.y + rect.height)
                .flat_map(|y| (rect.x..rect.x + rect.width).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let index = ((y * size.0 + x) * 4) as usize;
                    pixels[index..index + 4] == rgba(115, 113, 94, 255)
                })
                .collect::<Vec<_>>();
            assert!(!matches.is_empty(), "{target:?}");
            assert!(matches.iter().all(|&(x, y)| x >= rect.x + 6
                && x < rect.x + 22
                && y >= rect.y + 6
                && y < rect.y + 22));
        }
    }

    #[test]
    fn item_colors_match_reference_without_changing_control_colors() {
        for (rarity, rgb) in [
            ("Normal", [200, 200, 200]),
            ("Magic", [136, 136, 255]),
            ("Rare", [255, 255, 119]),
            ("Unique", [175, 96, 37]),
            ("Gem", [27, 162, 155]),
            ("Currency", [170, 158, 130]),
            ("Quest", [74, 230, 58]),
        ] {
            assert_eq!(rarity_color(rarity), rgba(rgb[0], rgb[1], rgb[2], 255));
        }

        let mut view = View::new().unwrap();
        let size = (2560, 1440);
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        view.paint(
            &mut pixels,
            Frame {
                size,
                scale: 1.0,
                zoom: 1.0,
                phase: 0.0,
            },
            &fixture(),
            &Interaction::default(),
        )
        .unwrap();
        for color in [
            rgba(127, 127, 127, 255),
            rgba(255, 255, 255, 255),
            rgba(255, 255, 119, 255),
            rgba(159, 157, 138, 255),
        ] {
            assert!(
                pixels.chunks_exact(4).any(|pixel| pixel == color),
                "{color:?}"
            );
        }
    }

    #[test]
    fn menus_capture_hits_and_scale_without_clipping() {
        let mut view = View::new().unwrap();
        let model = fixture();
        for (size, scale) in [
            ((1920, 1080), 1.0),
            ((2560, 1440), 1.25),
            ((1280, 720), 1.5),
        ] {
            for menu in [
                Menu::Settings,
                Menu::Rarity,
                Menu::Status,
                Menu::Currency,
                Menu::Indexed,
            ] {
                let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
                let mut interaction = Interaction {
                    menu: Some(menu),
                    ..Interaction::default()
                };
                view.paint(
                    &mut pixels,
                    Frame {
                        size,
                        scale,
                        zoom: scale,
                        phase: 0.0,
                    },
                    &model,
                    &interaction,
                )
                .unwrap();
                assert!(view.bounds.x + view.bounds.width <= size.0);
                assert!(view.bounds.y + view.bounds.height <= size.1);
                let popup = view
                    .hits
                    .iter()
                    .find(|h| h.target == Target::Popup)
                    .unwrap()
                    .rect;
                assert_eq!(
                    view.hit((popup.x + 1) as f64, (popup.y + 1) as f64),
                    Some(Target::Popup)
                );
                assert_eq!(
                    view.hit((view.panel.x + 1) as f64, (view.panel.y + 1) as f64),
                    Some(Target::DismissMenu)
                );
                if menu != Menu::Settings {
                    let count = model.evaluation.as_ref().unwrap().search_options[menu.key()].len();
                    assert_eq!(
                        view.hits
                            .iter()
                            .filter(|h| matches!(h.target, Target::Option(_, _)))
                            .count(),
                        count
                    );
                }
                for hit in view.hits.iter().filter(|h| {
                    matches!(
                        h.target,
                        Target::Option(_, _) | Target::ScaleUp | Target::ScaleDown
                    )
                }) {
                    let x = (hit.rect.x + hit.rect.width / 2) as f64;
                    let y = (hit.rect.y + hit.rect.height / 2) as f64;
                    assert!(popup.contains(x, y));
                    assert_eq!(view.hit(x, y), Some(hit.target));
                }
                assert!(interaction.close_popup());
                assert!(!interaction.close_popup());
            }
        }
    }

    #[test]
    fn quota_strip_scales_and_repairs_only_its_region() {
        use std::time::{Duration, Instant};
        for scale in [1.0, 1.25, 1.5] {
            let size = (2560, 1440);
            let mut view = View::new().unwrap();
            let mut model = fixture();
            let now = Instant::now();
            model.budget.observe(
                super::super::budget::Snapshot {
                    observed_at_ms: 1,
                    retry_after_ms: 10_000,
                    windows: vec![],
                },
                now - Duration::from_millis(5300),
            );
            model.budget.tick(now - Duration::from_millis(300));
            model.retry_at = model.budget.retry_at();
            let mut pixels = vec![0; (size.0 * size.1 * 4) as usize];
            view.paint(
                &mut pixels,
                Frame {
                    size,
                    scale,
                    zoom: scale,
                    phase: 0.0,
                },
                &model,
                &Interaction::default(),
            )
            .unwrap();
            let rect = view
                .hits
                .iter()
                .find(|hit| hit.target == Target::RateBudget)
                .unwrap()
                .rect;
            assert_eq!(rect.width, px(400, scale));
            assert_eq!(rect.height, px(3, scale));
            let pixel = |x| {
                let i = (((rect.y + rect.height / 2) * size.0 + x) * 4) as usize;
                &pixels[i..i + 4]
            };
            assert_eq!(pixel(rect.x + rect.width / 4), rgba(11, 11, 11, 255));
            assert_eq!(pixel(rect.x + 3 * rect.width / 4), rgba(118, 109, 78, 255));
            assert_eq!(view.budget_damage(view.budget_fill), Damage::None);
            assert!(matches!(view.budget_damage((0.4, true)), Damage::Rect(_)));
        }
    }

    #[test]
    fn ready_and_results_have_distinct_sections() {
        let view = View::new().unwrap();
        let mut model = fixture();
        let rows = view.rows(&model, 1.0);
        let ready = view
            .layout((1920, 1080), (16, 32), 1.0, &rows, 0, true, &model)
            .unwrap();
        assert!(ready.find_bounds(Node::Search).is_some());
        assert!(ready.find_bounds(Node::Listings).is_none());
        model.results = Some(Default::default());
        let results = view
            .layout((1920, 1080), (16, 32), 1.0, &rows, 0, true, &model)
            .unwrap();
        assert!(results.find_bounds(Node::Search).is_none());
        assert_eq!(
            results.bounds(Node::Table).height + results.bounds(Node::Listings).height,
            300
        );
        assert_eq!(ready.bounds(Node::Header), results.bounds(Node::Header));
    }

    #[test]
    fn result_link_refresh_and_unresolved_rows_keep_distinct_scaled_targets() {
        for scale in [1.0, 1.25, 1.5] {
            let mut view = View::new().unwrap();
            let mut model = fixture();
            model.evaluation.as_mut().unwrap().item.modifiers[0].supported = false;
            model.evaluation.as_mut().unwrap().item.modifiers[0].resolution =
                Some("unknown".into());
            model.results = Some(Default::default());
            let size = (2560, 1440);
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            view.paint(
                &mut pixels,
                Frame {
                    size,
                    scale,
                    zoom: scale,
                    phase: 0.0,
                },
                &model,
                &Interaction::default(),
            )
            .unwrap();
            for target in [
                Target::TradeLink,
                Target::Search,
                Target::ReadOnlyModifier(0),
            ] {
                let rect = view
                    .hits
                    .iter()
                    .find(|hit| hit.target == target)
                    .unwrap()
                    .rect;
                assert_eq!(
                    view.hit(
                        (rect.x + rect.width / 2) as f64,
                        (rect.y + rect.height / 2) as f64
                    ),
                    Some(target)
                );
            }
            assert!(!view.hits.iter().any(|hit| matches!(
                hit.target,
                Target::Minimum(0) | Target::Maximum(0) | Target::Modifier(0)
            )));
            let refresh = view
                .hits
                .iter()
                .find(|hit| hit.target == Target::Search)
                .unwrap()
                .rect;
            assert_eq!(refresh.width, px(16, scale));
            assert_eq!(refresh.height, px(16, scale));
        }
    }

    #[test]
    fn leaving_the_panel_releases_editing_without_clearing_the_search() {
        let mut interaction = Interaction {
            focus: Some(Target::Minimum(0)),
            hover: Some(Target::Minimum(0)),
            tooltip: true,
            scroll: 3,
            position: Some((400, 200)),
            ..Interaction::default()
        };
        interaction.leave_panel();
        assert_eq!(interaction.focus, None);
        assert_eq!(interaction.hover, None);
        assert!(!interaction.tooltip);
        assert_eq!(interaction.scroll, 3);
        assert_eq!(interaction.position, Some((400, 200)));
    }

    #[test]
    fn focusing_only_accepts_included_fields_without_changing_the_query() {
        let mut model = fixture();
        model.filters[0].enabled = true;
        model.filters[0].min = "9".into();
        model.results = Some(Default::default());
        let mut interaction = Interaction::default();
        assert!(interaction.focus_field(Target::Minimum(0), &model));
        for target in [Target::Minimum(1), Target::Maximum(99), Target::Modifier(0)] {
            assert!(!interaction.focus_field(target, &model));
            assert_eq!(interaction.focus, Some(Target::Minimum(0)));
        }
        assert!(interaction.focus_field(Target::Maximum(0), &model));
        assert_eq!(model.filters[0].min, "9");
        assert!(!model.filters[1].enabled);
        assert_eq!(model.generation, 0);
        assert!(model.results.is_some());
    }

    #[test]
    fn wrapped_item_lines_fit_the_same_metrics_used_for_painting() {
        let view = View::new().unwrap();
        let text = "Conjure Azmeri wisps to engulf a Rare Beast for a duration, Hindering them. If you defeat the Beast while it is engulfed in wisps, it will be captured by this gem.";
        for scale in [1.0, 1.05, 1.2, 1.25, 1.5] {
            let size = ITEM_TEXT_SIZE * scale;
            let width = ((CONTENT - 32.0) * scale) as u32;
            for line in wrap(&view.item, text, size, width) {
                assert!(
                    text_width(&view.item, &line, size) <= width as f32,
                    "{scale}: {line}"
                );
            }
        }
    }

    #[test]
    fn item_font_matches_reference_face_metrics_at_fractional_scales() {
        let view = View::new().unwrap();
        for (text, size, width) in [
            ("Gloom Finger", 19.0, 122.118),
            ("Gold Ring", 19.0, 90.037),
            ("Modifiable", ITEM_TEXT_SIZE, 72.457),
        ] {
            for scale in [1.0, 1.05, 1.2, 1.25, 1.5] {
                let actual = text_width(&view.item, text, size * scale);
                assert!(
                    (actual - width * scale).abs() < 0.002,
                    "{text} at {scale}: {actual}"
                );
            }
        }
    }

    #[test]
    fn repeated_frames_and_returning_to_a_scale_reuse_resources() {
        let mut view = View::new().unwrap();
        let model = fixture();
        let mut interaction = Interaction::default();
        let mut pixels = vec![0; 1920 * 1080 * 4];
        for scale in [1.0, 1.25] {
            view.paint(
                &mut pixels,
                Frame {
                    size: (1920, 1080),
                    scale,
                    zoom: scale,
                    phase: 0.0,
                },
                &model,
                &interaction,
            )
            .unwrap();
        }
        let chrome_misses = view.artwork.cache_stats().misses;
        let glyph_misses = view.glyphs.stats().misses;
        for scale in [1.0, 1.25, 1.0] {
            interaction.hover = Some(Target::Search);
            view.paint(
                &mut pixels,
                Frame {
                    size: (1920, 1080),
                    scale,
                    zoom: scale,
                    phase: 0.0,
                },
                &model,
                &interaction,
            )
            .unwrap();
            assert_eq!(view.artwork.cache_stats().misses, chrome_misses);
            assert_eq!(view.glyphs.stats().misses, glyph_misses);
            assert!(
                view.artwork.cache_stats().cached_bytes <= view.artwork.cache_stats().budget_bytes
            );
            assert!(view.glyphs.stats().cached_bytes <= view.glyphs.stats().budget_bytes);
        }
    }

    #[test]
    fn queued_thread_counts_match_at_fractional_scales() {
        for scale in [1.0, 1.2, 1.25, 1.5, 2.0] {
            let size = (1920, 1080);
            let mut reference = None;
            for threads in [1, 2, 3, 4, 6, 8] {
                let mut view = View::new().unwrap();
                view.render_budget = std::num::NonZeroU32::new(threads).unwrap();
                let mut pixels = vec![0; (size.0 * size.1 * 4) as usize];
                view.paint(
                    &mut pixels,
                    Frame {
                        size,
                        scale,
                        zoom: scale,
                        phase: 0.0,
                    },
                    &fixture(),
                    &Interaction::default(),
                )
                .unwrap();
                if let Some(reference) = &reference {
                    assert!(
                        &pixels == reference,
                        "pixel mismatch: scale {scale}, threads {threads}"
                    );
                } else {
                    reference = Some(pixels);
                }
            }
        }
    }

    #[test]
    #[ignore = "make bench-overlays"]
    fn overlay_benchmark() {
        let mut cases = Vec::new();
        for size in [(1920, 1080), (2560, 1440)] {
            for scale in [1.0, 1.25, 1.5, 2.0] {
                for scenario in [
                    "warm-repaint",
                    "hover",
                    "hover-partial",
                    "listing-scroll",
                    "scale-change",
                ] {
                    if !crate::render_bench::selected(scenario) {
                        continue;
                    }
                    cases.push(crate::render_bench::measure(
                        scenario,
                        size,
                        scale,
                        || {
                            let data: serde_json::Value = serde_json::from_slice(include_bytes!(
                                "../../tests/fixtures/evaluate-reference.json"
                            ))
                            .unwrap();
                            let mut model = Model::new("Forbidden Rites".into());
                            model.evaluated(
                                0,
                                serde_json::from_value(data["evaluation"].clone()).unwrap(),
                            );
                            if scenario == "listing-scroll" {
                                let mut results: super::super::model::Results =
                                    serde_json::from_value(data["results"].clone()).unwrap();
                                results.listings = results
                                    .listings
                                    .iter()
                                    .cloned()
                                    .cycle()
                                    .take(results.listings.len() * 10)
                                    .collect();
                                model.results = Some(results);
                            }
                            let mut view = View::new().unwrap();
                            view.render_budget = crate::render_bench::render_threads();
                            (
                                view,
                                model,
                                Interaction::default(),
                                vec![0; (size.0 * size.1 * 4) as usize],
                            )
                        },
                        |(view, model, interaction, pixels), index| {
                            let previous = interaction.hover;
                            if matches!(scenario, "hover" | "hover-partial") {
                                interaction.hover = (index % 2 == 0).then_some(Target::Search);
                            }
                            if scenario == "listing-scroll" {
                                interaction.scroll = index % 10;
                            }
                            let scale = if scenario == "scale-change" && index % 2 == 0 {
                                scale * 1.05
                            } else {
                                scale
                            };
                            let repair = if scenario == "hover-partial" {
                                view.target_damage(previous).union(view.target_damage(interaction.hover))
                                    .bounds(size).unwrap()
                            } else {
                                Rect { x: 0, y: 0, width: size.0, height: size.1 }
                            };
                            view.paint_region(
                                pixels,
                                Frame {
                                    size,
                                    scale,
                                    zoom: scale,
                                    phase: 0.0,
                                },
                                model,
                                interaction,
                                repair,
                            )
                            .unwrap();
                        },
                        |(view, _, _, pixels)| {
                            let glyphs = view.glyphs.stats();
                            let chrome = view.artwork.cache_stats();
                            serde_json::json!({
                                "canvas_bytes": pixels.len(),
                                "repair_pixels": if scenario == "hover-partial" {
                                    view.target_damage(Some(Target::Search)).bounds(size)
                                        .map_or(0, |rect| u64::from(rect.width) * u64::from(rect.height))
                                } else {
                                    u64::from(size.0) * u64::from(size.1)
                                },
                                "render_threads": view.render_threads,
                                "glyph_cache": {
                                    "entries": glyphs.entries,
                                    "cached_bytes": glyphs.cached_bytes,
                                    "live_bytes": glyphs.live_bytes,
                                    "retained_bytes": glyphs.retained_bytes,
                                    "hits": glyphs.hits,
                                    "misses": glyphs.misses,
                                    "evictions": glyphs.evictions,
                                },
                                "chrome_cache": {
                                    "entries": chrome.entries,
                                    "cached_bytes": chrome.cached_bytes,
                                    "live_bytes": chrome.live_bytes,
                                    "retained_bytes": chrome.retained_bytes,
                                    "hits": chrome.hits,
                                    "misses": chrome.misses,
                                    "evictions": chrome.evictions,
                                },
                                "scale_kind": "UI scale at DPR 1",
                                "assets": "embedded chrome; no network",
                                "note": "forces paint; a clean live view would skip painting"
                            })
                        },
                    ));
                }
            }
        }
        crate::render_bench::report("poe2", cases);
    }

    fn fixture() -> Model {
        let data: serde_json::Value =
            serde_json::from_slice(include_bytes!("../../tests/fixtures/evaluate-preview.json"))
                .unwrap();
        let mut m = Model::new("Standard".into());
        m.evaluated(
            0,
            serde_json::from_value(data["evaluation"].clone()).unwrap(),
        );
        m
    }
    #[test]
    fn title_stays_draggable_beside_the_league_link() {
        let mut view = View::new().unwrap();
        for scale in [1.0, 1.25, 1.5] {
            let size = (1920, 1080);
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            view.paint(
                &mut pixels,
                Frame {
                    size,
                    scale,
                    zoom: scale,
                    phase: 0.0,
                },
                &fixture(),
                &Interaction::default(),
            )
            .unwrap();
            let league = view
                .hits
                .iter()
                .find(|hit| hit.target == Target::League)
                .unwrap()
                .rect;
            let y = (league.y + league.height / 2) as f64;
            assert_eq!(
                view.hit((league.x - px(12, scale)) as f64, y),
                Some(Target::Drag)
            );
            assert_eq!(
                view.hit((league.x + league.width / 2) as f64, y),
                Some(Target::League)
            );
            let regions = view.pointer_regions();
            assert!(!regions.iter().any(|rect| rect.contains(0.0, 0.0)));
            for hit in &view.hits {
                let x = (hit.rect.x + hit.rect.width / 2) as f64;
                let y = (hit.rect.y + hit.rect.height / 2) as f64;
                assert!(regions.iter().any(|rect| rect.contains(x, y)));
            }
        }
    }

    #[test]
    fn profile_controls_match_reference_padding_and_margins() {
        let mut view = View::new().unwrap();
        for scale in [1.0, 1.25, 1.5] {
            let size = (1920, 1080);
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            view.paint(
                &mut pixels,
                Frame {
                    size,
                    scale,
                    zoom: scale,
                    phase: 0.0,
                },
                &fixture(),
                &Interaction::default(),
            )
            .unwrap();
            let [exact, broad] = [
                (Target::Profile(Profile::Quick), "Quick Price"),
                (Target::Profile(Profile::Broad), "Broad (-10%)"),
            ]
            .map(|(target, label)| {
                let rect = view
                    .hits
                    .iter()
                    .find(|hit| hit.target == target)
                    .unwrap()
                    .rect;
                assert_eq!(rect.height, px(24, scale));
                assert_eq!(
                    rect.width,
                    text_width(&view.ui, label, 14.0 * scale).ceil() as u32 + px(24, scale)
                );
                assert_eq!(
                    view.hit(
                        (rect.x + px(12, scale)) as f64,
                        (rect.y + rect.height / 2) as f64
                    ),
                    Some(target)
                );
                rect
            });
            assert_eq!(broad.x - exact.x - exact.width, px(13, scale));
        }
    }

    #[test]
    fn crafting_base_profiles_fit_without_overlap() {
        let mut model = fixture();
        model.evaluation.as_mut().unwrap().item.profiles =
            vec![Profile::Base, Profile::Quick, Profile::Broad];
        model.apply_profile(Profile::Base);
        let mut view = View::new().unwrap();
        let size = (2560, 1440);
        for scale in [1.0, 1.25, 1.5] {
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            view.paint(
                &mut pixels,
                Frame {
                    size,
                    scale,
                    zoom: scale,
                    phase: 0.0,
                },
                &model,
                &Interaction::default(),
            )
            .unwrap();
            let hits: Vec<_> = view
                .hits
                .iter()
                .filter(|hit| matches!(hit.target, Target::Profile(_)))
                .collect();
            assert_eq!(hits.len(), 3);
            assert!(hits[0].rect.x >= view.bounds.x);
            assert!(hits[2].rect.x + hits[2].rect.width <= view.bounds.x + px(MAIN as u32, scale));
            for pair in hits.windows(2) {
                assert!(pair[0].rect.x + pair[0].rect.width < pair[1].rect.x);
            }
        }
    }

    #[test]
    fn profile_radio_uses_active_profile_after_filter_changes() {
        let mut view = View::new().unwrap();
        let size = (1920, 1080);
        for (profile, target) in [
            (Profile::Quick, Target::Profile(Profile::Quick)),
            (Profile::Broad, Target::Profile(Profile::Broad)),
        ] {
            let mut model = fixture();
            model.apply_profile(profile);
            for filter in &mut model.filters {
                filter.enabled = false;
            }
            model.invalidate();
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            view.paint(
                &mut pixels,
                Frame {
                    size,
                    scale: 1.0,
                    zoom: 1.0,
                    phase: 0.0,
                },
                &model,
                &Interaction::default(),
            )
            .unwrap();
            for hit in view
                .hits
                .iter()
                .filter(|hit| matches!(hit.target, Target::Profile(_)))
            {
                let index = ((hit.rect.y + 12) * size.0 + hit.rect.x + 12) as usize * 4;
                let pixel = &pixels[index..index + 4];
                let distance = pixel
                    .iter()
                    .zip(ACCENT)
                    .map(|(a, b)| a.abs_diff(b))
                    .max()
                    .unwrap();
                if hit.target == target {
                    assert!(distance <= 16, "selected center: {pixel:?}");
                } else {
                    assert!(distance > 64, "unselected center: {pixel:?}");
                }
            }
        }
    }

    #[test]
    fn scales_fields_and_keeps_hit_regions_aligned() {
        let mut view = View::new().unwrap();
        let mut model = fixture();
        model.filters[0].enabled = true;
        model.filters[2].enabled = true;
        for (size, scale) in [
            ((1920, 1080), 1.0),
            ((2560, 1440), 1.25),
            ((1280, 720), 1.5),
        ] {
            assert!(text_width(&view.ui, "200%", 11.0 * scale) <= px(48, scale) as f32 - 12.0);
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            view.paint(
                &mut pixels,
                Frame {
                    size,
                    scale,
                    zoom: scale,
                    phase: 0.0,
                },
                &model,
                &Interaction::default(),
            )
            .unwrap();
            assert!(view.bounds.x + view.bounds.width <= size.0);
            assert!(view.bounds.y + view.bounds.height <= size.1);
            for hit in &view.hits {
                assert!(hit.rect.x + hit.rect.width <= size.0);
                assert!(hit.rect.y + hit.rect.height <= size.1);
                assert!(hit.rect.x >= view.bounds.x);
                assert!(hit.rect.x + hit.rect.width <= view.bounds.x + view.bounds.width);
                if let Target::Minimum(i) = hit.target {
                    let row = view
                        .hits
                        .iter()
                        .find(|h| h.target == Target::Modifier(i))
                        .unwrap()
                        .rect;
                    assert!(
                        (row.y as i32 + row.height as i32 / 2
                            - hit.rect.y as i32
                            - hit.rect.height as i32 / 2)
                            .abs()
                            <= 1
                    );
                    assert_eq!(
                        view.hit(
                            (hit.rect.x + hit.rect.width / 2) as f64,
                            (hit.rect.y + hit.rect.height / 2) as f64
                        ),
                        Some(Target::Minimum(i))
                    );
                    let max = view
                        .hits
                        .iter()
                        .find(|h| h.target == Target::Maximum(i))
                        .unwrap()
                        .rect;
                    assert_eq!(
                        view.hit(
                            (hit.rect.x + hit.rect.width + max.x) as f64 / 2.0,
                            (hit.rect.y + hit.rect.height / 2) as f64,
                        ),
                        Some(Target::Include(i))
                    );
                    let outside_field = ((hit.rect.y * size.0 + hit.rect.x - 2) * 4) as usize;
                    assert_eq!(
                        pixels[outside_field + 3],
                        255,
                        "range controls need an opaque backing"
                    );
                }
            }
            for (current, backwards, expected) in [
                (Target::Minimum(0), false, Some(Target::Maximum(0))),
                (Target::Maximum(0), false, Some(Target::Minimum(2))),
                (Target::Minimum(2), true, Some(Target::Maximum(0))),
                (Target::Maximum(2), false, Some(Target::Minimum(0))),
                (Target::Minimum(0), true, Some(Target::Maximum(2))),
                (Target::Minimum(1), false, None),
            ] {
                assert_eq!(view.next_field(Some(current), backwards, &model), expected);
            }
            assert!(!model.filters[1].enabled);
            assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
        }
        for filter in &mut model.filters {
            filter.enabled = false;
        }
        assert_eq!(
            view.next_field(Some(Target::Minimum(0)), false, &model),
            None
        );
    }
    #[test]
    fn item_column_is_inset_and_header_tracks_line_count() {
        let view = View::new().unwrap();
        let model = fixture();
        for scale in [1.0, 1.25] {
            let rows = view.rows(&model, scale);
            for (two_titles, height) in [(false, 34), (true, 54)] {
                let layout = view
                    .layout((1920, 1080), (16, 32), scale, &rows, 0, two_titles, &model)
                    .unwrap();
                let main = layout.bounds(Node::Main);
                let header = layout.bounds(Node::Header);
                assert_eq!(header.x - main.x, px(12, scale));
                assert_eq!(header.width, px(400, scale));
                assert!((header.height as i32 - px(height, scale) as i32).abs() <= 1);
                assert_eq!(header.x, layout.bounds(Node::Search).x);
            }
        }
    }
    #[test]
    fn scrolling_never_moves_search_or_changes_panel_height() {
        let view = View::new().unwrap();
        let mut model = fixture();
        let original = model.evaluation.as_ref().unwrap().item.modifiers.clone();
        for _ in 0..3 {
            model
                .evaluation
                .as_mut()
                .unwrap()
                .item
                .modifiers
                .extend(original.clone());
        }
        let rows = view.rows(&model, 1.25);
        let first = view
            .layout((2560, 1440), (100, 32), 1.25, &rows, 0, true, &model)
            .unwrap();
        for scroll in [1, 10, rows.len() - 1] {
            let next = view
                .layout((2560, 1440), (100, 32), 1.25, &rows, scroll, true, &model)
                .unwrap();
            assert_eq!(first.bounds(Node::Search), next.bounds(Node::Search));
            assert_eq!(first.bounds(Node::Root), next.bounds(Node::Root));
        }
    }
}
