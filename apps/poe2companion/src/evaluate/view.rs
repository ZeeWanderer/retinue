use super::artwork::Artwork;
use super::model::{Item, Model, Profile, number};
use retinue_overlay::{
    Rect,
    fontdue::{Font, FontSettings},
    layout::{UiLayout, UiTree},
    line_edit::{Colors, LineEdit},
    painter::{Painter, TextLine, css_rgba as rgba, text_width, wrap_text},
    taffy::{self, prelude::*},
};

mod plaques;
mod results;

const MAIN: f32 = 424.0;
const CONTENT: f32 = 400.0;
const RAIL: f32 = 132.0;
const ITEM_TEXT_SIZE: f32 = 14.4;
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
    ui: Font,
    ui_bold: Font,
    ui_semibold: Font,
    item: Font,
    artwork: Artwork,
    pub hits: Vec<Hit>,
    pub bounds: Rect,
    pub panel: Rect,
    pub item_rows: usize,
    pub visible_listings: usize,
    pub visible_options: usize,
    pub editor: LineEdit,
    pub preview_time: Option<u64>,
    scale: f32,
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
        Ok(Self {
            ui: Font::from_bytes(
                include_bytes!("../../assets/Lato-Regular.ttf").as_slice(),
                FontSettings::default(),
            )?,
            ui_bold: Font::from_bytes(
                include_bytes!("../../assets/Lato-Bold.ttf").as_slice(),
                FontSettings::default(),
            )?,
            ui_semibold: Font::from_bytes(
                include_bytes!("../../assets/Lato-Semibold.ttf").as_slice(),
                FontSettings::default(),
            )?,
            item: Font::from_bytes(
                include_bytes!(concat!(env!("OUT_DIR"), "/Fontin-SmallCaps.ttf")).as_slice(),
                FontSettings::default(),
            )?,
            artwork: Artwork::new()?,
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
        let Frame {
            size,
            scale,
            zoom,
            phase,
        } = render;
        let mut p = Painter::new(pixels, size.0, size.1).map_err(|e| e.to_string())?;
        p.clear();
        self.hits.clear();
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
        let main = layout.bounds(Node::Main);
        self.panel = main;
        self.bounds = layout.bounds(Node::Root);
        fill(&mut p, main, rgba(0, 0, 0, 225));
        let bar = layout.bounds(Node::Bar);
        fill(&mut p, bar, rgba(28, 27, 23, 255));
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
                        height: px(8, s),
                        ..r
                    },
                    rarity,
                    s,
                );
                r.y += px(8, s);
                r.height = r.height.saturating_sub(px(8, s));
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
        self.artwork.divider(
            &mut p,
            Rect {
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
                y: profiles.y + px(8, s) + profiles.height.saturating_sub(px(32, s)) / 2,
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
            self.paint_estimate(&mut p, rect, model, s);
        }
        self.paint_search(&mut p, &layout, model, interaction, s, phase);
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
                        Some("Estimate in Exalted Orbs; not a guaranteed sale price")
                    }
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
        p.finish().map_err(|e| e.to_string())
    }

    fn paint_search(
        &mut self,
        p: &mut Painter<'_>,
        layout: &UiLayout<Node>,
        model: &Model,
        interaction: &Interaction,
        s: f32,
        phase: f32,
    ) {
        let unavailable = model
            .evaluation
            .as_ref()
            .is_some_and(|e| e.item.trade_unavailable.is_some());
        if let Some(search) = layout.find_bounds(Node::Search) {
            let hovered = !unavailable && interaction.hover == Some(Target::Search);
            let highlighted = hovered || model.loading;
            let background = if highlighted {
                rgba(35, 34, 29, 255)
            } else {
                rgba(22, 21, 18, 255)
            };
            let border = if model.loading {
                ACCENT
            } else {
                rgba(118, 109, 78, 255)
            };
            let mut inner_border = border;
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
            if !model.loading && !unavailable {
                self.hits
                    .extend(std::iter::once(search).chain(ornaments).map(|rect| Hit {
                        target: Target::Search,
                        rect,
                    }));
            }
            let label = if model.loading {
                if model.evaluation.is_some() {
                    "Searching..."
                } else {
                    "Reading item..."
                }
            } else if model.evaluation.is_none() {
                "Retry"
            } else {
                "Search"
            };
            let text_width = text_width(&self.ui_bold, label, 14.0 * s).ceil() as u32;
            p.draw_text_vertically_centered(
                &self.ui_bold,
                TextLine::new(
                    search.x + search.width.saturating_sub(text_width) / 2,
                    search.y,
                    search.height.saturating_sub(u(s)),
                ),
                14.0 * s,
                label,
                if unavailable { MUTED } else { ACCENT },
            );
            if model.loading {
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
            let refresh = Rect {
                x: inner.x + inner.width - px(16, s),
                y: status.y + status.height.saturating_sub(px(16, s)) / 2,
                width: px(16, s),
                height: px(16, s),
            };
            self.icon_button(p, refresh, Target::Search, Icon::Refresh, interaction, s);
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
        p: &mut Painter<'_>,
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
        p: &mut Painter<'_>,
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
        p: &mut Painter<'_>,
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
            self.editor.paint(
                p,
                field.inset(px(3, s)),
                12.0 * s,
                Colors {
                    text: INK,
                    selection: rgba(70, 63, 43, 255),
                },
            );
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
    ) -> Result<UiLayout<Node>, String> {
        let mut t = UiTree::new();
        let bar = t.leaf(Node::Bar, fixed(MAIN * s, 28.0 * s))?;
        let header_height = if two_titles { 54.0 } else { 34.0 };
        let header = t.leaf(Node::Header, fixed(CONTENT * s, header_height * s))?;
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
        let viewport = (rows.iter().map(|r| r.height() * s).sum::<f32>() + 12.0 * s)
            .min(max_item)
            .max(38.0 * s);
        let mut used = 0.0;
        let mut item_nodes = Vec::new();
        for (i, row) in rows.iter().enumerate().skip(scroll) {
            let h = row.height() * s;
            if used + h > viewport - 12.0 * s {
                break;
            }
            item_nodes.push(t.leaf(Node::Row(i), fixed(CONTENT * s, h))?);
            used += h;
        }
        let item = t.column(
            Node::Item,
            Style {
                padding: taffy::Rect {
                    top: length(6.0 * s),
                    bottom: length(6.0 * s),
                    left: length(0.0_f32),
                    right: length(0.0_f32),
                },
                ..fixed(CONTENT * s, viewport)
            },
            &item_nodes,
        )?;
        let profiles = t.leaf(Node::Profiles, fixed(CONTENT * s, 36.0 * s))?;
        let card = t.column(Node::Card, Style::default(), &[header, item, profiles])?;
        let mut nodes = vec![card];
        if estimate_height > 0.0 {
            let mut style = fixed(CONTENT * s, estimate_height * s);
            style.margin.top = length(8.0 * s);
            nodes.push(t.leaf(Node::Estimate, style)?);
        }
        if error_height > 0.0 {
            let mut style = fixed(CONTENT * s, error_height * s);
            style.margin.top = length(8.0 * s);
            nodes.push(t.leaf(Node::Error, style)?);
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
                Node::Search | Node::Status | Node::ResultFilters | Node::Table
            ) {
                style.margin.top = length(8.0 * s);
            }
            nodes.push(t.leaf(key, style)?);
        }
        let content = t.column(
            Node::Content,
            Style {
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
            &nodes,
        )?;
        let main = t.column(
            Node::Main,
            Style {
                size: Size {
                    width: length(MAIN * s),
                    height: auto(),
                },
                ..Default::default()
            },
            &[bar, content],
        )?;
        let root = t.row(
            Node::Root,
            Style {
                size: Size {
                    width: length((MAIN + RAIL) * s),
                    height: auto(),
                },
                ..Default::default()
            },
            &[main],
        )?;
        t.compute(root, origin, size)
    }
    fn button(
        &mut self,
        p: &mut Painter<'_>,
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
        p: &mut Painter<'_>,
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
        p: &mut Painter<'_>,
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
    fn item_label(&self, p: &mut Painter<'_>, rect: Rect, text: &str, size: f32, color: [u8; 4]) {
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
        self.lines.len() as f32 * 20.0 + if self.separator { 8.0 } else { 0.0 }
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
fn fill(p: &mut Painter<'_>, r: Rect, c: [u8; 4]) {
    p.fill_rounded_rect(r.x, r.y, r.width, r.height, 0, c);
}
fn surface(
    p: &mut Painter<'_>,
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
fn frame(p: &mut Painter<'_>, r: Rect, c: [u8; 4], w: u32) {
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
            assert_eq!(estimate.y, card.y + card.height + px(8, scale));
            assert_eq!(error.y, estimate.y + estimate.height + px(8, scale));
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
