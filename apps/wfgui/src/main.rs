mod data;
mod foundry;
mod login;
mod media;
mod pages;
mod preferences;
mod session;
mod settings;
mod shell;
mod style;
mod widgets;

use clap::Parser;
use retinue_client::paths::Paths;
use retinue_ui::{
    damage::Damage,
    desktop::{
        self, Application,
        winit::{
            dpi::LogicalSize,
            event::WindowEvent,
            window::{Window, WindowAttributes},
        },
    },
    painter::{Painter, render_thread_budget},
};
use retinue_ui::{preparation::Preparation, thumbnails::Key as ImageKey};
use shell::{Page, Shell};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};

#[derive(Parser)]
#[command(about = "Warframe desktop GUI", version)]
struct Args {
    #[arg(long, help = "Report paths as JSON without opening a window")]
    paths_json: bool,
    #[arg(long, default_value = "foundry", value_parser = ["settings", "foundry", "mastery", "inventory", "relics", "market", "builds"])]
    page: String,
    #[arg(long, help = "Override the saved UI scale for this run", value_parser = clap::value_parser!(u16).range(75..=200))]
    ui_scale: Option<u16>,
    #[arg(long, default_value = "1440x900", value_parser = parse_size)]
    size: (u32, u32),
    #[arg(long, help = "Use deterministic data without contacting the daemon")]
    fixture: bool,
    #[arg(
        long,
        conflicts_with = "window_screenshot",
        help = "Render the page offscreen and exit"
    )]
    screenshot: Option<PathBuf>,
    #[arg(
        long,
        default_value = "window",
        requires = "screenshot",
        help = "Capture window, player, a rail, page.grid/card/summary, or a control ID"
    )]
    capture_target: String,
    #[arg(
        long,
        default_value = "",
        requires = "screenshot",
        help = "Page mode or Build Planner tab to capture"
    )]
    view: String,
    #[arg(
        long,
        default_value = "",
        requires = "screenshot",
        help = "Equipment/group name or build ID to capture (read-only)"
    )]
    select: String,
    #[arg(
        long,
        requires = "screenshot",
        help = "Show an item's order book in the right rail"
    )]
    market_item: Option<String>,
    #[arg(
        long,
        requires = "screenshot",
        help = "Hover a page control ID and capture its expanded state"
    )]
    hover: Option<String>,
    #[arg(long, help = "Open the Foundry filter panel")]
    show_filters: bool,
    #[arg(long)]
    left_collapsed: bool,
    #[arg(long)]
    right_collapsed: bool,
    #[arg(long, requires = "screenshot", value_parser = ["left", "right", "both", "category", "search"])]
    capture_animation: Option<String>,
    #[arg(long, default_value_t = 1, requires = "capture_animation", value_parser = clap::value_parser!(u8).range(0..=7))]
    capture_category_from: u8,
    #[arg(long, default_value_t = 85, requires = "capture_animation", value_parser = clap::value_parser!(u16).range(0..=1000))]
    capture_time_ms: u16,
    #[arg(long, help = "Capture the first ready desktop frame and exit")]
    window_screenshot: Option<PathBuf>,
    #[arg(long, help = "Write the displayed data as JSON")]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = 0)]
    scroll: u32,
    #[arg(long, default_value = "")]
    search: String,
    #[arg(long,default_value_t=1,value_parser=clap::value_parser!(u8).range(0..=7))]
    category: u8,
}

impl Args {
    fn save_preferences(&self) -> bool {
        !self.fixture && self.screenshot.is_none() && self.window_screenshot.is_none()
    }

    fn preferences(&self, paths: &Paths) -> (preferences::Preferences, Option<String>) {
        let path = preferences::path(paths);
        let (mut values, error) = if self.save_preferences() {
            match preferences::Values::load(&path) {
                Ok(values) => (values, None),
                Err(error) => (
                    preferences::Values::default(),
                    Some(format!(
                        "Could not read preferences from {}: {error}. Saved preferences were left unchanged.",
                        path.display(),
                    )),
                ),
            }
        } else {
            (preferences::Values::default(), None)
        };
        if let Some(scale) = self.ui_scale {
            values.ui_scale = scale;
        }
        (preferences::Preferences::new(values), error)
    }
}

fn parse_size(value: &str) -> Result<(u32, u32), String> {
    let (width, height) = value.split_once('x').ok_or("Expected WIDTHxHEIGHT")?;
    let width = width.parse::<u32>().map_err(|_| "Invalid width")?;
    let height = height.parse::<u32>().map_err(|_| "Invalid height")?;
    if !(960..=7680).contains(&width) || !(600..=4320).contains(&height) {
        return Err("Size outside 960x600..7680x4320".into());
    }
    Ok((width, height))
}

fn main() -> std::process::ExitCode {
    match start(Args::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("wfgui: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn start(args: Args) -> Result<(), String> {
    if args.paths_json {
        println!("{}", data::path_report(&Paths::from_env()));
        return Ok(());
    }
    if (args.show_filters || args.capture_target.starts_with("foundry.")) && args.page != "foundry"
    {
        return Err("Foundry captures require --page foundry".into());
    }
    let paths = Paths::from_env();
    let (preferences, preference_error) = args.preferences(&paths);
    let scale = preferences.scale();
    let page = match args.page.as_str() {
        "mastery" => Page::Mastery,
        "inventory" => Page::Inventory,
        "relics" => Page::Relics,
        "market" => Page::Market,
        "builds" => Page::Builds,
        "settings" => Page::Settings,
        _ => Page::Foundry,
    };
    if let Some(path) = &args.screenshot {
        let snapshot = if args.fixture {
            data::Snapshot::fixture()
        } else {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?
                .block_on(data::execute(
                    &Paths::from_env(),
                    data::Job::Refresh,
                    &AtomicBool::new(false),
                ))
                .snapshot
        };
        let mut view = Shell::new(Some(snapshot), scale)?;
        view.resize(args.size, 1.0)?;
        view.set_collapsed(args.left_collapsed, args.right_collapsed);
        view.set_page(page);
        prepare_capture(&mut view, &args)?;
        if let Some(animation) = args.capture_animation.as_deref() {
            let elapsed = std::time::Duration::from_millis(args.capture_time_ms as u64);
            if animation == "category" {
                view.foundry
                    .set_category(args.capture_category_from as usize);
                let now = Instant::now();
                view.foundry.select_category(args.category as usize, now);
                view.foundry.tick(now + elapsed);
            } else if animation == "search" {
                view.foundry.focus_action(Some(foundry::Action::Search));
                view.foundry.tick(Instant::now() + elapsed);
            } else {
                view.sample_animation(animation != "right", animation != "left", elapsed);
            }
        }
        let mut pixels = vec![0_u8; args.size.0 as usize * args.size.1 as usize * 4];
        let mut painter = Painter::with_threads(
            &mut pixels,
            args.size.0,
            args.size.1,
            render_thread_budget(4),
        )
        .map_err(|e| e.to_string())?;
        painter.fill_rounded_rect(0, 0, args.size.0, args.size.1, 0, shell::BACKGROUND);
        view.paint(&mut painter);
        painter.finish().map_err(|e| e.to_string())?;
        let image = retinue_ui::image::RgbImage::from_fn(args.size.0, args.size.1, |x, y| {
            let index = (y as usize * args.size.0 as usize + x as usize) * 4;
            retinue_ui::image::Rgb([pixels[index + 2], pixels[index + 1], pixels[index]])
        });
        let bounds = view.capture_bounds(&args.capture_target)?;
        retinue_ui::image::imageops::crop_imm(
            &image,
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
        )
        .to_image()
        .save(path)
        .map_err(|e| e.to_string())?;
        report_view(args.report.as_ref(), &view)?;
        println!("{}", path.display());
        return Ok(());
    }
    let options = desktop::Options {
        capture: args.window_screenshot.clone(),
        exit_after_capture: args.window_screenshot.is_some(),
    };
    desktop::run(
        |sender| {
            let mut view =
                Shell::with_preferences(args.fixture.then(data::Snapshot::fixture), preferences)?;
            if let Some(error) = &preference_error {
                eprintln!("wfgui: {error}");
            }
            view.settings.set_preference_error(preference_error);
            let preference_writer = if args.save_preferences() {
                let output = sender.clone();
                match preferences::Writer::new(preferences::path(&paths), move |result| {
                    output.send(Message::Preferences(result));
                }) {
                    Ok(writer) => Some(writer),
                    Err(error) => {
                        let error = format!("Preferences cannot be saved: {error}");
                        eprintln!("wfgui: {error}");
                        view.settings.set_preference_error(Some(error));
                        None
                    }
                }
            } else {
                None
            };
            view.set_collapsed(args.left_collapsed, args.right_collapsed);
            view.set_page(page);
            if args.fixture {
                view.foundry.update(foundry::model::fixture());
                view.pages.fixture();
            }
            view.foundry.set_category(args.category as usize);
            view.foundry.set_search(&args.search);
            view.foundry.jump(args.scroll as f32);
            if args.show_filters || args.capture_target == "foundry.filters" {
                view.foundry.open_filters();
            }
            let worker = if args.fixture {
                None
            } else {
                let output = sender.clone();
                Some(data::Worker::new(move |update| {
                    output.send(Message::Settings(update))
                })?)
            };
            if let Some(worker) = &worker {
                worker.submit(data::Job::Refresh)?;
            }
            if args.fixture {
                write_report(args.report.as_ref(), &view.settings.snapshot)?;
            }
            let output = sender.clone();
            let images =
                foundry::images::spawn((!args.fixture).then(data::native_cache_root), move || {
                    output.send(Message::Images);
                })?;
            let mut gui = Gui {
                clipboard: None,
                view,
                preference_writer,
                worker,
                size: args.size,
                report: args.report,
                capture: args.window_screenshot.is_some(),
                sender,
                foundry_worker: None,
                session: None,
                login: None,
                page_clipboard: None,
                images,
                image_keys: Vec::new(),
                asset_ids: Vec::new(),
                refresh: 0,
                fixture: args.fixture,
                memory_generation: 0,
                images_ready: false,
            };
            gui.dispatch();
            Ok(gui)
        },
        options,
    )
}

fn write_report(path: Option<&PathBuf>, snapshot: &data::Snapshot) -> Result<(), String> {
    if let Some(path) = path {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(snapshot).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn report_view(path: Option<&PathBuf>, view: &Shell) -> Result<(), String> {
    if view.page == Page::Settings {
        return write_report(path, &view.settings.snapshot);
    }
    if view.page != Page::Foundry {
        if let Some(path) = path {
            let mut report = view.pages.report();
            report["center_size"] =
                serde_json::json!([view.geometry.center.width, view.geometry.center.height]);
            report["image_errors"] = serde_json::json!(view.foundry.image_errors);
            report["asset_errors"] = serde_json::json!(view.foundry.asset_errors);
            report["rank_image_loaded"] = serde_json::json!(view.foundry.rank_image().is_some());
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    if let Some(path) = path {
        let stats = view.foundry.cache_stats;
        let work = view.foundry.image_work;
        let data = serde_json::json!({"revision":view.foundry.snapshot.revision,"total":view.foundry.snapshot.items.len(),"filtered":view.foundry.count(),"columns":view.foundry.grid.columns,"offset":view.foundry.offset,"maximum_scroll":view.foundry.maximum(),"viewport":[view.foundry.viewport.width,view.foundry.viewport.height],"prepared_images":view.foundry.prepared_count(),"source_bytes":stats.0.live_bytes,"thumbnail_bytes":stats.1.live_bytes,"image_errors":view.foundry.image_errors,"asset_errors":view.foundry.asset_errors,"error":view.foundry.error,"cache_warnings":view.foundry.cache_warnings,"image_work":{"ram_hits":work.ram_hits,"disk_hits":work.disk_hits,"resamples":work.resamples,"persistent_resamples":work.persistent_resamples,"disk_writes":work.disk_writes}});
        let mut data = data;
        data["center_size"] =
            serde_json::json!([view.geometry.center.width, view.geometry.center.height]);
        data["search"] = serde_json::json!(view.foundry.query());
        data["input_error"] = serde_json::json!(view.foundry.input_error);
        data["pages"] = view.pages.report();
        data["layout"] = view.foundry.layout_report();
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn prepare_capture(view: &mut Shell, args: &Args) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let mut channel = if args.fixture {
            None
        } else {
            Some(session::connect().await?)
        };
        let snapshot = if let Some(channel) = &mut channel {
            foundry::worker::snapshot(channel).await?
        } else {
            foundry::model::fixture()
        };
        view.foundry.update(snapshot);
        if let Some(channel) = &mut channel {
            view.pages.update(session::Update::Connected);
            for _ in 0..256 {
                let Some(mut request) = view.pages.requests.pop_front() else {
                    break;
                };
                if request.body["op"] == "market_quote" {
                    request.body["cache_only"] = serde_json::json!(true);
                }
                let mut body = request.body.clone();
                body["id"] = serde_json::json!(50);
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(20),
                    channel.request(&body),
                )
                .await
                .map_err(|_| "Capture request timed out".to_owned())
                .and_then(|v| v.map_err(|e| e.to_string()))
                .and_then(|v| session::response(&v));
                view.pages.capture_reply(request, result);
            }
        } else {
            view.pages.fixture();
        }
        view.pages.capture_state(
            &args.view,
            &args.select,
            &args.search,
            args.market_item.as_deref(),
        )?;
        if std::mem::take(&mut view.pages.market_reveal) {
            view.show_market_rail();
            view.set_collapsed(args.left_collapsed, false);
        }
        if let Some(channel) = &mut channel {
            for _ in 0..256 {
                let Some(mut request) = view.pages.requests.pop_front() else {
                    break;
                };
                if request.body["op"] == "market_quote" {
                    request.body["cache_only"] = serde_json::json!(true);
                }
                let mut body = request.body.clone();
                body["id"] = serde_json::json!(51);
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(20),
                    channel.request(&body),
                )
                .await
                .map_err(|_| "Capture request timed out".to_owned())
                .and_then(|v| v.map_err(|e| e.to_string()))
                .and_then(|v| session::response(&v));
                view.pages.capture_reply(request, result);
            }
        }
        view.pages.capture_scroll(args.scroll);
        view.foundry.set_category(args.category as usize);
        view.foundry.set_search(&args.search);
        view.foundry.jump(args.scroll as f32);
        if args.show_filters || args.capture_target == "foundry.filters" {
            view.foundry.open_filters();
        }
        if args.capture_target == "foundry.component" && !view.foundry.open_first_component() {
            return Err("No visible component to capture".into());
        }
        if let Some(key) = &args.hover {
            view.pages.capture_hover(key)?;
        }
        if let Some(channel) = &mut channel {
            let mut desired = view.foundry.desired_assets();
            desired.extend(view.pages.assets());
            for assets in desired.chunks(32) {
                channel
                    .send(&serde_json::json!({"op":"asset_resolve","id":4,"assets":assets}))
                    .await
                    .map_err(|e| e.to_string())?;
                tokio::time::timeout(std::time::Duration::from_secs(30), async {
                    loop {
                        let value = channel.receive().await.map_err(|e| e.to_string())?;
                        if value["event"] == "asset" {
                            if let Some(asset) = foundry::worker::descriptor(&value["data"]) {
                                view.foundry.assets(vec![asset]);
                            }
                        }
                        if value["id"] == 4 {
                            if value["ok"] != true {
                                return Err("Asset request failed".to_owned());
                            }
                            view.foundry
                                .asset_errors
                                .extend(foundry::worker::asset_errors(
                                    value["data"]["assets"]
                                        .as_array()
                                        .ok_or("Invalid asset response")?,
                                ));
                            let descriptors = value["data"]["assets"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(foundry::worker::descriptor)
                                .collect::<Vec<_>>();
                            view.foundry.assets(descriptors.clone());
                            view.pages.descriptors(descriptors);
                            return Ok(());
                        }
                    }
                })
                .await
                .map_err(|_| "Asset request timed out".to_owned())??;
            }
        }
        Ok::<_, String>(())
    })?;
    let mut cache = foundry::images::cache((!args.fixture).then(data::native_cache_root));
    let mut images = Vec::new();
    let mut errors = Vec::new();
    let mut requests = view.foundry.image_requests();
    requests.extend(view.pages.image_requests());
    cache.begin_batch(&requests);
    for request in requests {
        match cache.prepare(&request) {
            Ok(image) => {
                cache.persist(&request, &image);
                images.push((request.key, image));
            }
            Err(error) => errors.push(format!("{:?}: {error}", request.source)),
        }
    }
    view.pages.images(&images);
    view.foundry.accept_images(images, errors);
    view.foundry.cache_stats = cache.stats();
    view.foundry.image_work = cache.work;
    view.foundry.cache_warnings = cache.take_warnings();
    Ok(())
}

struct Gui {
    clipboard: Option<desktop::clipboard::Clipboard<foundry::Transfer>>,
    page_clipboard: Option<desktop::clipboard::Clipboard<(bool, String, Instant)>>,
    view: Shell,
    preference_writer: Option<preferences::Writer>,
    worker: Option<data::Worker>,
    size: (u32, u32),
    report: Option<PathBuf>,
    capture: bool,
    sender: desktop::Sender<Message>,
    foundry_worker: Option<foundry::worker::Worker>,
    session: Option<session::Worker>,
    login: Option<login::Login>,
    images: Preparation<foundry::images::Input, foundry::images::Batch>,
    image_keys: Vec<ImageKey>,
    asset_ids: Vec<foundry::model::Asset>,
    refresh: u64,
    fixture: bool,
    memory_generation: u64,
    images_ready: bool,
}

enum Message {
    Clipboard(foundry::Transfer, Result<Option<String>, String>),
    PageClipboard((bool, String, Instant), Result<Option<String>, String>),
    Session(session::Update),
    Login(Result<serde_json::Value, String>),
    Preferences(Result<(), String>),
    Settings(data::Update),
    Foundry(foundry::worker::Update),
    Images,
}

impl Gui {
    fn dispatch(&mut self) {
        if std::mem::take(&mut self.view.pages.login_requested) {
            if let Some(login) = self.login.take() {
                drop(login);
            } else if !self.fixture {
                let sender = self.sender.clone();
                match login::Login::start(move |result| {
                    sender.send(Message::Login(result));
                }) {
                    Ok(login) => {
                        self.login = Some(login);
                        self.view.pages.login_active = true;
                    }
                    Err(error) => self.view.pages.login_result(Err(error)),
                }
            }
        }
        if self.session.is_none() && !self.fixture {
            let sender = self.sender.clone();
            match session::Worker::new(move |update| sender.send(Message::Session(update))) {
                Ok(worker) => self.session = Some(worker),
                Err(error) => self.view.pages.error = Some(error),
            }
        }
        if let Some(destination) = self.view.foundry.destination.take() {
            match destination {
                foundry::Destination::Market { item, buyers } => {
                    self.view
                        .pages
                        .open_market(item, if buyers { "buy" } else { "sell" });
                }
                foundry::Destination::Relics(query) => {
                    self.view.pages.navigation = Some((Page::Relics, query))
                }
            }
        }
        if std::mem::take(&mut self.view.pages.market_reveal) {
            self.view.show_market_rail();
        }
        if let Some((page, query)) = self.view.pages.navigation.take() {
            self.view.set_page(page);
            if page == Page::Foundry {
                self.view.foundry.set_search(&query);
            } else if !query.is_empty() {
                self.view.pages.search(page, &query);
            }
        }
        if let Some(worker) = &self.session {
            while let Some(request) = self.view.pages.requests.pop_front() {
                if let Err(error) = worker.submit(request.clone()) {
                    self.view.pages.update(session::Update::Reply {
                        request,
                        result: Err(error),
                    });
                }
            }
        }
        for rail in [false, true] {
            let request = if rail {
                self.view.pages.rail.clipboard.take()
            } else {
                self.view.pages.ui.clipboard.take()
            };
            if let Some((key, stamp, operation)) = request {
                if self.page_clipboard.is_none() {
                    let sender = self.sender.clone();
                    match desktop::clipboard::Clipboard::new(move |token, result| {
                        sender.send(Message::PageClipboard(token, result));
                    }) {
                        Ok(worker) => self.page_clipboard = Some(worker),
                        Err(error) => self.view.pages.error = Some(error),
                    }
                }
                if let Some(worker) = &self.page_clipboard {
                    if let Err(error) = worker.submit((rail, key, stamp), operation) {
                        self.view.pages.error = Some(error);
                    }
                }
            }
        }
        if let Some(url) = self.view.pages.external.take() {
            if url.starts_with("https://warframe.market/")
                || url.starts_with("https://overframe.gg/")
            {
                std::thread::spawn(move || {
                    let _ = std::process::Command::new("xdg-open").arg(url).status();
                });
            }
        }
        if let Some((token, operation)) = self.view.foundry.clipboard_request.take() {
            if self.clipboard.is_none() {
                let sender = self.sender.clone();
                match desktop::clipboard::Clipboard::new(move |token, result| {
                    sender.send(Message::Clipboard(token, result));
                }) {
                    Ok(clipboard) => self.clipboard = Some(clipboard),
                    Err(error) => {
                        self.sender.send(Message::Clipboard(token, Err(error)));
                    }
                }
            }
            if let Some(clipboard) = &self.clipboard
                && let Err(error) = clipboard.submit(token, operation)
            {
                self.sender.send(Message::Clipboard(token, Err(error)));
            }
        }
        let changes = self.view.preferences.take_changes();
        if let Some(writer) = &self.preference_writer
            && let Err(error) = writer.submit(changes)
        {
            self.sender.send(Message::Preferences(Err(error)));
        }
        if self.memory_generation != self.view.settings.memory_generation {
            self.memory_generation = self.view.settings.memory_generation;
            self.view.foundry.clear_images();
            self.view.pages.media.clear();
            self.image_keys.clear();
            self.images_ready = false;
            if let Err(error) = self.images.request(foundry::images::Input {
                generation: self.memory_generation,
                requests: Vec::new(),
            }) {
                self.view.settings.error = Some(error.to_string());
            }
        }
        if let Some(job) = self.view.settings.pending.take() {
            let result = self
                .worker
                .as_ref()
                .ok_or("Preview is read-only".to_owned())
                .and_then(|worker| worker.submit(job));
            if let Err(error) = result {
                self.view.settings.update(data::Update {
                    snapshot: self.view.settings.snapshot.clone(),
                    error: Some(error),
                });
            }
        }
        {
            if self.foundry_worker.is_none() && !self.fixture {
                let output = self.sender.clone();
                match foundry::worker::Worker::new(move |update| {
                    output.send(Message::Foundry(update))
                }) {
                    Ok(worker) => self.foundry_worker = Some(worker),
                    Err(error) => self.view.foundry.error = Some(error),
                }
            }
            let mut assets = self.view.foundry.desired_assets();
            assets.extend(self.view.pages.assets());
            let mut seen = std::collections::HashSet::new();
            assets.retain(|a| seen.insert(a.id.clone()));
            let ids = assets.clone();
            if ids != self.asset_ids || self.refresh != self.view.foundry.refresh {
                self.asset_ids = ids;
                self.refresh = self.view.foundry.refresh;
                if let Some(worker) = &self.foundry_worker {
                    worker.set(foundry::worker::Desired {
                        assets,
                        refresh: self.refresh,
                    });
                }
            }
            let mut requests = self.view.foundry.image_requests();
            requests.extend(self.view.pages.image_requests());
            let mut seen = std::collections::HashSet::new();
            requests.retain(|r| seen.insert(r.key));
            let keys = requests.iter().map(|r| r.key).collect::<Vec<_>>();
            if keys != self.image_keys {
                self.image_keys = keys;
                self.images_ready = false;
                self.view.foundry.image_errors.clear();
                self.view.foundry.cache_warnings.clear();
                if let Err(error) = self.images.request(foundry::images::Input {
                    generation: self.memory_generation,
                    requests,
                }) {
                    self.view.foundry.error = Some(error.to_string());
                }
            }
        }
    }
}

impl Application for Gui {
    type Message = Message;
    fn attributes(&self) -> WindowAttributes {
        use desktop::winit::platform::{
            wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11,
        };
        let minimum = self.view.minimum_size();
        let attributes = Window::default_attributes()
            .with_active(!self.capture)
            .with_title("Warframe")
            .with_decorations(false)
            .with_inner_size(LogicalSize::new(
                self.size.0.max(minimum.0),
                self.size.1.max(minimum.1),
            ))
            .with_min_inner_size(LogicalSize::new(minimum.0, minimum.1));
        let icon =
            retinue_ui::image::load_from_memory(include_bytes!("../resources/app/wfgui-256.png"))
                .expect("embedded app icon")
                .into_rgba8();
        let (width, height) = icon.dimensions();
        let attributes = attributes.with_window_icon(
            desktop::winit::window::Icon::from_rgba(icon.into_raw(), width, height).ok(),
        );
        let attributes = WindowAttributesExtWayland::with_name(attributes, "wfgui", "wfgui");
        WindowAttributesExtX11::with_name(attributes, "wfgui", "wfgui")
    }
    fn resize(&mut self, size: (u32, u32), display_scale: f64) {
        if let Err(error) = self.view.resize(size, display_scale) {
            self.view.settings.error = Some(error);
        }
    }
    fn background(&self) -> [u8; 4] {
        shell::BACKGROUND
    }
    fn ready(&self) -> bool {
        let page_ready = match self.view.page {
            Page::Settings => self.view.settings.ready,
            Page::Foundry => {
                self.view.foundry.ready
                    && (self.view.foundry.error.is_some() || self.view.foundry.assets_ready())
                    && self.images_ready
            }
            _ => self.view.pages.ready() && self.images_ready,
        };
        page_ready && (self.view.geometry.right.is_empty() || self.view.pages.rail_capture_ready())
    }
    fn close_requested(&self) -> bool {
        self.view.close_requested
    }
    fn paint(&mut self, painter: &mut Painter<'_>) -> Result<(), String> {
        self.view.paint(painter);
        Ok(())
    }
    fn tick(&mut self, now: Instant) -> Damage {
        let damage = self.view.tick(now);
        self.dispatch();
        damage
    }
    fn deadline(&self) -> Option<Instant> {
        self.view.deadline()
    }
    fn message(&mut self, update: Message) -> Damage {
        let damage = match update {
            Message::Login(result) => {
                self.login = None;
                self.view.pages.login_result(result);
                Damage::Full
            }
            Message::Session(update) => {
                self.view.pages.update(update);
                Damage::Full
            }
            Message::PageClipboard((rail, key, stamp), result) => {
                match result {
                    Ok(value) => {
                        if rail {
                            self.view.pages.rail.clipboard_reply(&key, stamp, value);
                        } else {
                            self.view.pages.ui.clipboard_reply(&key, stamp, value);
                        }
                    }
                    Err(error) => self.view.pages.error = Some(error),
                }
                Damage::Full
            }
            Message::Clipboard(token, result) => {
                self.view.foundry.clipboard_reply(token, result);
                self.view.page_damage(Damage::Full)
            }
            Message::Preferences(result) => {
                let error = result.err();
                if self.view.settings.preference_error != error {
                    self.view.settings.set_preference_error(error);
                    Damage::Full
                } else {
                    Damage::None
                }
            }
            Message::Settings(update) => {
                self.view.settings.update(update);
                if self.view.page == Page::Settings {
                    Damage::Full
                } else {
                    Damage::None
                }
            }
            Message::Foundry(update) => {
                let damage = match update {
                    foundry::worker::Update::Snapshot(snapshot) => {
                        self.view.foundry.update(snapshot)
                    }
                    foundry::worker::Update::Assets(assets) => {
                        self.view.pages.descriptors(assets.clone());
                        self.view.foundry.assets(assets);
                        self.image_keys.clear();
                        Damage::None
                    }
                    foundry::worker::Update::AssetErrors(errors) => {
                        self.view.pages.media.errors.extend(errors.clone());
                        self.view.foundry.asset_errors.extend(errors);
                        Damage::None
                    }
                    foundry::worker::Update::Error(error) => {
                        self.view.foundry.error = Some(error);
                        self.view.foundry.ready = true;
                        Damage::Full
                    }
                };
                let _ = damage;
                Damage::Full
            }
            Message::Images => match self.images.poll() {
                Ok(Some(batch)) => {
                    self.images_ready = batch.done;
                    self.view.foundry.cache_stats = batch.stats;
                    self.view.foundry.image_work = batch.work;
                    for warning in batch.warnings {
                        let warnings = &mut self.view.foundry.cache_warnings;
                        if warnings.len() < 16 && !warnings.contains(&warning) {
                            warnings.push(warning);
                        }
                    }
                    self.view.pages.images(&batch.images);
                    self.view.pages.image_health(&batch.images, &batch.errors);
                    let page_damage = self.view.image_damage(&batch.images);
                    let damage = self.view.foundry.accept_images(
                        batch.images,
                        batch.errors.into_iter().map(|(_, reason)| reason).collect(),
                    );
                    self.view
                        .settings
                        .set_image_memory(self.view.foundry.memory());
                    if self.view.page == Page::Foundry {
                        page_damage.union(self.view.page_damage(damage))
                    } else {
                        page_damage
                    }
                }
                Err(error) => {
                    self.view.foundry.error = Some(error.to_string());
                    self.images_ready = true;
                    self.view.page_damage(Damage::Full)
                }
                _ => Damage::None,
            },
        };
        self.dispatch();
        if let Err(error) = report_view(self.report.as_ref(), &self.view) {
            self.view.settings.error = Some(error);
        }
        damage
    }
    fn accessibility(&self) -> desktop::accesskit::TreeUpdate {
        self.view.accessibility()
    }
    fn access(&mut self, request: desktop::accesskit::ActionRequest, window: &Window) -> Damage {
        self.view.access(request);
        self.view.apply_window_action(window);
        self.dispatch();
        Damage::Full
    }

    fn event(&mut self, event: &WindowEvent, window: &Window) -> Damage {
        let damage = self.view.event(event, window);
        self.dispatch();
        if matches!(
            event,
            WindowEvent::KeyboardInput { .. } | WindowEvent::MouseInput { .. }
        ) {
            if let Err(error) = report_view(self.report.as_ref(), &self.view) {
                self.view.settings.error = Some(error);
            }
        }
        damage
    }
}
