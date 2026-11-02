use super::*;
use crate::presentation::foundry::Action;
use retinue_client::transport::Channel;
use retinue_render::thumbnails::Thumbnails;
use std::io::Write;

pub(super) struct Residency {
    pub shell: Shell,
    pub runtime: tokio::runtime::Runtime,
    pub channel: Option<Channel>,
    cache: Thumbnails,
    descriptors: std::collections::HashSet<String>,
    pixels: Vec<u8>,
    output: std::io::BufWriter<std::fs::File>,
    samples: usize,
    cycle: usize,
}

impl Residency {
    pub fn new(live: bool, output: &str) -> Self {
        let mut shell = Shell::new(Some(Snapshot::fixture()), 1.25).unwrap();
        shell.resize((2560, 1440), 1.0).unwrap();
        shell.foundry.update(crate::data::foundry::fixture());
        shell.pages.fixture();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let channel = live.then(|| runtime.block_on(crate::domain::session::connect()).unwrap());
        let mut test = Self {
            shell,
            runtime,
            channel,
            cache: crate::presentation::foundry::images::cache(
                live.then(crate::domain::storage::native_cache_root),
            ),
            descriptors: Default::default(),
            pixels: vec![0; 2560 * 1440 * 4],
            output: std::io::BufWriter::new(std::fs::File::create(output).unwrap()),
            samples: 0,
            cycle: 0,
        };
        test.output.write_all(b"[\n").unwrap();
        test.refresh();
        test
    }

    pub fn refresh(&mut self) {
        let Some(channel) = &mut self.channel else {
            return;
        };
        let foundry = self
            .runtime
            .block_on(residency_packet(channel, json!({"op":"foundry_view"})));
        self.shell.foundry.update(foundry.decode().unwrap());
        for (key, body) in [
            ("inventory", json!({"op":"inventory_view"})),
            ("mastery", json!({"op":"mastery_view"})),
            (
                "relics",
                json!({"op":"relic_planner","era":"all","only_owned":false,"fetch_prices":false,"limit":"all"}),
            ),
            ("activity", json!({"op":"activity_view"})),
            ("equipment", json!({"op":"build_equipment"})),
            ("groups", json!({"op":"build_group_list"})),
            ("market", json!({"op":"market_account"})),
        ] {
            let reply = self
                .runtime
                .block_on(residency_packet(channel, body.clone()));
            self.shell.pages.capture_native_reply(
                crate::domain::session::Request {
                    key: key.into(),
                    body,
                },
                crate::domain::session::Reply::decode(key, reply),
            );
        }
    }

    pub fn present(&mut self, action: &str) {
        let start = Instant::now();
        self.shell.tick(Instant::now() + Duration::from_secs(1));
        if let Some(channel) = &mut self.channel {
            let mut assets = self.shell.foundry.desired_assets();
            assets.extend(self.shell.pages.assets());
            assets.retain(|a| self.descriptors.insert(a.id.clone()));
            for batch in assets.chunks(32) {
                let reply = self.runtime.block_on(residency_request(
                    channel,
                    json!({"op":"asset_resolve","assets":batch}),
                ));
                let descriptors: Vec<_> = reply["assets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(crate::domain::foundry::descriptor)
                    .collect();
                self.shell.foundry.assets(descriptors.clone());
                self.shell.pages.descriptors(descriptors);
            }
        }
        let mut requests = self.shell.foundry.image_requests();
        requests.extend(self.shell.pages.image_requests());
        self.cache.begin_batch(&requests);
        let images: Vec<_> = requests
            .iter()
            .map(|r| (r.key, self.cache.prepare(r).unwrap()))
            .collect();
        for (request, (_, image)) in requests.iter().zip(&images) {
            self.cache.persist(request, image);
        }
        self.shell.pages.images(&images);
        self.shell.foundry.accept_images(images, Vec::new());
        self.shell.foundry.cache_stats = self.cache.stats();
        let mut painter = Painter::with_threads(
            &mut self.pixels,
            2560,
            1440,
            crate::app::benchmark::threads(),
        )
        .unwrap();
        self.shell.paint(&mut painter);
        painter.finish().unwrap();
        let prepare_paint_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut report = crate::app::diagnostics::snapshot(&self.shell);
        report["page"] = json!(format!("{:?}", self.shell.page));
        report["cycle"] = json!(self.cycle);
        report["action"] = json!(action);
        report["prepare_paint_ms"] = json!(prepare_paint_ms);
        report["resamples"] = json!(self.cache.work.resamples);
        report["disk_hits"] = json!(self.cache.work.disk_hits);
        if self.samples > 0 {
            self.output.write_all(b",\n").unwrap();
        }
        serde_json::to_writer(&mut self.output, &report).unwrap();
        self.samples += 1;
    }

    fn signal(&mut self, key: &str, value: Option<&str>) {
        self.shell.pages.residency_signal(key, value);
        self.present(key);
    }

    pub fn tour(&mut self, cycle: usize) {
        self.cycle = cycle;
        for page in [
            Page::Foundry,
            Page::Mastery,
            Page::Inventory,
            Page::Relics,
            Page::Builds,
            Page::Market,
            Page::Settings,
        ] {
            self.shell.set_page(page);
            self.present("open");
            match page {
                Page::Foundry => {
                    for category in 0..8 {
                        self.shell.foundry.set_category(category);
                        self.present("category");
                        for offset in [500.0, 2000.0, 5000.0, 0.0] {
                            self.shell.foundry.jump(offset);
                            self.present("scroll");
                        }
                    }
                    self.shell.foundry.set_category(0);
                    for search in ["Em", "Ember", "Prime", ""] {
                        self.shell.foundry.set_search(search);
                        self.present("search");
                    }
                    for action in [Action::Filters, Action::ClearFilters, Action::CloseFilters] {
                        self.shell.foundry.activate(action);
                        self.present("filters");
                    }
                }
                Page::Mastery | Page::Inventory | Page::Relics => {
                    let categories: &[&str] = match page {
                        Page::Inventory => &["parts", "mods", "arcanes", "misc", "sets", "parts"],
                        Page::Relics => &["all", "lith", "meso", "neo", "axi", "requiem", "all"],
                        _ => &["all", "warframes", "weapons", "companions", "all"],
                    };
                    for category in categories {
                        self.signal(&format!("list:category:{category}"), None);
                        for offset in [500, 2000, 5000, 0] {
                            self.signal("list:scroll", Some(&offset.to_string()));
                        }
                    }
                    for search in ["Em", "Ember", "Prime", ""] {
                        self.signal("list:search", Some(search));
                    }
                    if page != Page::Relics {
                        for key in [
                            "list:filters",
                            "list:filter:prime:yes",
                            "list:filter:mastered:no",
                            "list:filters:clear",
                            "list:filters:close",
                        ] {
                            self.signal(key, None);
                        }
                    }
                    if self
                        .shell
                        .pages
                        .capture_state("context-menu", "", "", None)
                        .is_ok()
                    {
                        self.present("context-menu");
                        self.shell.pages.dismiss_context_menu();
                        self.present("dismiss-context");
                    }
                    if page == Page::Mastery {
                        for mode in ["relics", "platinum", "easy"] {
                            self.signal(&format!("list:mode:{mode}"), None);
                        }
                    }
                }
                Page::Builds => {
                    for tab in ["equipment", "discover", "groups", "equipment"] {
                        self.signal(&format!("build:tab:{tab}"), None);
                        self.shell.pages.wheel(false, 160.0, false);
                        self.present("scroll");
                    }
                    for query in ["Boar", "Mesa", ""] {
                        self.signal("build:search", Some(query));
                    }
                }
                Page::Market => {
                    for search in ["Ember", "Prime", ""] {
                        self.signal("market:search", Some(search));
                    }
                }
                _ => (),
            }
            for control in [Control::Left, Control::Right, Control::Left, Control::Right] {
                self.shell.activate(control, Instant::now());
                self.present("rail-collapse");
            }
            for key in [
                "rail:tab:market",
                "rail:tab:activity",
                "rail:fissure:steel",
                "rail:fissure:all",
            ] {
                self.signal(key, None);
            }
        }
        self.shell
            .settings
            .set_scale(if cycle % 2 == 0 { 1.5 } else { 1.25 });
        self.shell.resize((2560, 1440), 1.0).unwrap();
        self.present("scale");
    }

    pub fn finish(mut self) {
        self.output.write_all(b"\n]\n").unwrap();
        self.output.flush().unwrap();
    }
}
