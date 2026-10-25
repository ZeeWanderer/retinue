mod assets;
mod capture;
mod daemon;
mod desktop;
mod ducats;
mod external;
mod game_metadata;
mod incident;
mod inventory;
mod observer;
mod overlay;
mod paths;
mod preview;
mod relic;
mod runtime;
mod shortcut;
mod ui;

#[cfg(test)]
#[path = "../../../tools/render-bench/support.rs"]
mod render_bench;

use runtime::presentation::Event as UiEvent;
use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = r#"wfcompanion - Linux/Proton Warframe observer and overlay

Usage:
  wfcompanion
  wfcompanion launch -- COMMAND [ARG...]
  wfcompanion probe
  wfcompanion screenshot OUTPUT.png
  wfcompanion relic-ocr [IMAGE]
  wfcompanion preview list [--animated]
  wfcompanion preview image TYPE [--background IMAGE] [--scan IMAGE|--era ERA] OUTPUT.png
  wfcompanion preview image all OUTPUT_DIR
  wfcompanion preview video TYPE [--background IMAGE] OUTPUT.webm
  wfcompanion preview video all OUTPUT_DIR
  wfcompanion paths
  wfcompanion logs
  wfcompanion --relic-screenshot IMAGE

Commands:
  launch             Run Warframe command and keep companion alive with it
  probe              Print detected Warframe process state as JSON
  screenshot         Capture Warframe through KWin
  relic-ocr          Print OCR candidates from saved or newly captured image
  preview            Render overlays onto a transparent output-sized image
  paths              Report per-user directories as JSON
  logs               Print incident log path and recent entries

Options:
  -h, --help         Show this help
  --relic-screenshot Start overlay and process a saved image (developer mode)

Environment:
  WFCOMPANION_CAPTURE_DIR      Shared temporary capture directory
  WFCOMPANION_RELIC_SCREENSHOT Saved image used instead of live capture
  WFCOMPANION_TESSERACT        Tesseract executable path
  WFCOMPANION_DEBUG_BRIDGE     Proton DBWIN helper path
  WFCOMPANION_PREVIEW_SIZE     Preview size override as WIDTHxHEIGHT
  WFCOMPANION_KSCREEN_DOCTOR   kscreen-doctor executable path
  WFCOMPANION_FFMPEG           ffmpeg executable path
  WFCOMPANION_LOG              Incident log path
  WFCOMPANION_DEV_RELOAD       Set to 0 to disable automatic development reload
"#;

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Overlay {
        launch: Option<Vec<String>>,
        relic_screenshot: Option<PathBuf>,
    },
    Probe,
    Screenshot(PathBuf),
    RelicOcr(Option<PathBuf>),
    Preview(PreviewRequest),
    Paths,
    Logs,
    Help,
}

#[derive(Debug, Eq, PartialEq)]
enum PreviewRequest {
    List {
        animated_only: bool,
    },
    One {
        name: String,
        path: PathBuf,
        background: Option<PathBuf>,
        source: Option<PreviewSource>,
    },
    All {
        directory: PathBuf,
    },
    Animate {
        name: String,
        path: PathBuf,
        background: Option<PathBuf>,
    },
    AnimateAll {
        directory: PathBuf,
    },
}

#[derive(Debug, Eq, PartialEq)]
enum PreviewSource {
    RewardScreenshot(PathBuf),
    RelicInventory(String),
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments == ["--reload-check"] {
        return if runtime::reload::CONFIG
            .check::<runtime::reload::State>()
            .is_ok()
        {
            print!("{}", runtime::reload::CONFIG.ready);
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    // Startup is single-threaded; consume the handoff before spawning any services.
    let resumed = match unsafe { runtime::reload::CONFIG.resume::<runtime::reload::State>() } {
        Ok(resumed) => resumed,
        Err(error) => {
            eprintln!("wfcompanion: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut incident_writer = match incident::Writer::start() {
        Ok(writer) => Some(writer),
        Err(error) => {
            eprintln!("wfcompanion: {error}");
            return ExitCode::FAILURE;
        }
    };
    let _ = retinue_client::executable_path();
    match parse_command(&arguments)
        .and_then(|command| run_command(command, resumed, &mut incident_writer))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            incident::error("command.failed", &error);
            eprintln!("wfcompanion: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_command(
    command: Command,
    resumed: Option<runtime::reload::Handoff>,
    log: &mut Option<incident::Writer>,
) -> Result<(), String> {
    match command {
        Command::Help => {
            print!("{HELP}");
            Ok(())
        }
        Command::Probe => {
            println!(
                "{}",
                serde_json::to_string(&retinue_warframe::game_observer::find_warframe()).unwrap()
            );
            Ok(())
        }
        Command::Logs => incident::print_recent(100),
        Command::Paths => paths::print(),
        Command::Screenshot(path) => {
            let (width, height) = capture::save(&path)?;
            println!("saved {} ({width}x{height})", path.display());
            Ok(())
        }
        Command::RelicOcr(path) => {
            let report = match path {
                Some(path) => relic::diagnose(&path)?,
                None => {
                    let image = capture::capture()?;
                    relic::diagnose_image(&image, serde_json::Value::String("capture".to_owned()))?
                }
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| format!("could not format OCR report: {error}"))?
            );
            Ok(())
        }
        Command::Preview(PreviewRequest::List { animated_only }) => {
            for name in preview::names(animated_only) {
                println!("{name}");
            }
            Ok(())
        }
        Command::Preview(request) => {
            let dimensions = preview::current_dimensions()?;
            for path in preview::render(request, dimensions)? {
                println!(
                    "saved {} ({}x{})",
                    path.display(),
                    dimensions.0,
                    dimensions.1
                );
            }
            Ok(())
        }
        Command::Overlay {
            launch,
            relic_screenshot,
        } => runtime::run(launch, relic_screenshot, resumed, log),
    }
}

fn parse_command(arguments: &[String]) -> Result<Command, String> {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Ok(Command::Overlay {
            launch: None,
            relic_screenshot: None,
        });
    };
    match command {
        "-h" | "--help" | "help" if arguments.len() == 1 => Ok(Command::Help),
        "--probe" | "probe" if arguments.len() == 1 => Ok(Command::Probe),
        "screenshot" => parse_screenshot_command(&arguments[1..]),
        "relic-ocr" => parse_relic_ocr_command(&arguments[1..]),
        "preview" => parse_preview_command(&arguments[1..]),
        "paths" if arguments.len() == 1 => Ok(Command::Paths),
        "logs" if arguments.len() == 1 => Ok(Command::Logs),
        "--relic-screenshot" if arguments.len() == 2 => Ok(Command::Overlay {
            launch: None,
            relic_screenshot: Some(PathBuf::from(&arguments[1])),
        }),
        "launch" => parse_launch_command(&arguments[1..]),
        _ => Err(format!(
            "invalid arguments: {}; run `wfcompanion --help`",
            arguments.join(" ")
        )),
    }
}

fn parse_preview_command(arguments: &[String]) -> Result<Command, String> {
    match arguments {
        [command] if command == "list" => Ok(Command::Preview(PreviewRequest::List {
            animated_only: false,
        })),
        [command, option] if command == "list" && option == "--animated" => {
            Ok(Command::Preview(PreviewRequest::List {
                animated_only: true,
            }))
        }
        [kind, target, directory] if kind == "image" && target == "all" => {
            Ok(Command::Preview(PreviewRequest::All {
                directory: PathBuf::from(directory),
            }))
        }
        [kind, target, directory] if kind == "video" && target == "all" => {
            Ok(Command::Preview(PreviewRequest::AnimateAll {
                directory: PathBuf::from(directory),
            }))
        }
        [kind, name, rest @ ..] if kind == "image" => {
            let (path, background, source) = parse_preview_output(rest, true)?;
            validate_preview_source(name, source.as_ref())?;
            Ok(Command::Preview(PreviewRequest::One {
                name: name.clone(),
                path,
                background,
                source,
            }))
        }
        [kind, name, rest @ ..] if kind == "video" => {
            let (path, background, _) = parse_preview_output(rest, false)?;
            Ok(Command::Preview(PreviewRequest::Animate {
                name: name.clone(),
                path,
                background,
            }))
        }
        _ => Err(
            "preview requires list [--animated], image TYPE [--background IMAGE] OUTPUT.png, "
                .to_owned()
                + "image all OUTPUT_DIR, video TYPE [--background IMAGE] OUTPUT.webm, "
                + "or video all OUTPUT_DIR",
        ),
    }
}

fn parse_preview_output(
    arguments: &[String],
    allow_source: bool,
) -> Result<(PathBuf, Option<PathBuf>, Option<PreviewSource>), String> {
    let mut output = None;
    let mut background = None;
    let mut source = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--background" if index + 1 < arguments.len() => {
                background = Some(PathBuf::from(&arguments[index + 1]));
                index += 2;
            }
            "--background" => return Err("--background requires an image path".to_owned()),
            "--scan" if allow_source && index + 1 < arguments.len() && source.is_none() => {
                source = Some(PreviewSource::RewardScreenshot(PathBuf::from(
                    &arguments[index + 1],
                )));
                index += 2;
            }
            "--era" if allow_source && index + 1 < arguments.len() && source.is_none() => {
                source = Some(PreviewSource::RelicInventory(arguments[index + 1].clone()));
                index += 2;
            }
            "--scan" | "--era" if source.is_some() => {
                return Err("preview accepts only one live data source".to_owned());
            }
            "--scan" => return Err("--scan requires an image path".to_owned()),
            "--era" => return Err("--era requires a relic era".to_owned()),
            option if option.starts_with('-') => {
                return Err(format!("unknown preview option: {option}"));
            }
            value if output.is_none() => {
                output = Some(PathBuf::from(value));
                index += 1;
            }
            value => return Err(format!("unexpected preview argument: {value}")),
        }
    }
    output
        .map(|path| (path, background, source))
        .ok_or_else(|| "preview requires an output path".to_owned())
}

fn validate_preview_source(name: &str, source: Option<&PreviewSource>) -> Result<(), String> {
    match (name, source) {
        ("relic-rewards", None | Some(PreviewSource::RewardScreenshot(_)))
        | ("relic-suggestions", None | Some(PreviewSource::RelicInventory(_)))
        | (_, None) => Ok(()),
        (_, Some(PreviewSource::RewardScreenshot(_))) => {
            Err("--scan is only valid for relic-rewards".to_owned())
        }
        (_, Some(PreviewSource::RelicInventory(_))) => {
            Err("--era is only valid for relic-suggestions".to_owned())
        }
    }
}

fn parse_screenshot_command(arguments: &[String]) -> Result<Command, String> {
    match arguments {
        [path] if !path.starts_with('-') => Ok(Command::Screenshot(PathBuf::from(path))),
        [] => Err("screenshot requires an output path".to_owned()),
        [option, ..] if option.starts_with('-') => {
            Err(format!("unknown screenshot option: {option}"))
        }
        _ => Err("screenshot accepts one output path".to_owned()),
    }
}

fn parse_relic_ocr_command(arguments: &[String]) -> Result<Command, String> {
    match arguments {
        [] => Ok(Command::RelicOcr(None)),
        [path] if !path.starts_with('-') => Ok(Command::RelicOcr(Some(PathBuf::from(path)))),
        [option, ..] if option.starts_with('-') => {
            Err(format!("unknown relic-ocr option: {option}"))
        }
        _ => Err("relic-ocr accepts one image path".to_owned()),
    }
}

fn parse_launch_command(arguments: &[String]) -> Result<Command, String> {
    let mut command = arguments.to_vec();
    if command.first().is_some_and(|arg| arg == "--") {
        command.remove(0);
    }
    if command.is_empty() {
        Err("launch requires command after --".to_owned())
    } else {
        Ok(Command::Overlay {
            launch: Some(command),
            relic_screenshot: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn parses_steam_launch_wrapper() {
        assert_eq!(
            parse_command(&arguments(&["launch", "--", "/game", "arg"])),
            Ok(Command::Overlay {
                launch: Some(arguments(&["/game", "arg"])),
                relic_screenshot: None,
            })
        );
    }

    #[test]
    fn parses_diagnostic_commands() {
        assert_eq!(parse_command(&arguments(&["paths"])), Ok(Command::Paths));
        assert_eq!(
            parse_command(&arguments(&["screenshot", "capture.png"])),
            Ok(Command::Screenshot(PathBuf::from("capture.png")))
        );
        assert_eq!(
            parse_command(&arguments(&[
                "preview",
                "image",
                "relic-rewards",
                "preview.png"
            ])),
            Ok(Command::Preview(PreviewRequest::One {
                name: "relic-rewards".to_owned(),
                path: PathBuf::from("preview.png"),
                background: None,
                source: None,
            }))
        );
        assert_eq!(
            parse_command(&arguments(&["preview", "image", "all", "previews"])),
            Ok(Command::Preview(PreviewRequest::All {
                directory: PathBuf::from("previews"),
            }))
        );
        assert_eq!(
            parse_command(&arguments(&[
                "preview",
                "video",
                "relic-loading",
                "--background",
                "screen.jpg",
                "preview.webm",
            ])),
            Ok(Command::Preview(PreviewRequest::Animate {
                name: "relic-loading".to_owned(),
                path: PathBuf::from("preview.webm"),
                background: Some(PathBuf::from("screen.jpg")),
            }))
        );
        assert_eq!(
            parse_command(&arguments(&["relic-ocr", "rewards.jpg"])),
            Ok(Command::RelicOcr(Some(PathBuf::from("rewards.jpg"))))
        );
    }

    #[test]
    fn parses_live_preview_sources() {
        assert_eq!(
            parse_command(&arguments(&[
                "preview",
                "image",
                "relic-rewards",
                "--scan",
                "rewards.jpg",
                "--background",
                "rewards.jpg",
                "preview.png",
            ])),
            Ok(Command::Preview(PreviewRequest::One {
                name: "relic-rewards".to_owned(),
                path: PathBuf::from("preview.png"),
                background: Some(PathBuf::from("rewards.jpg")),
                source: Some(PreviewSource::RewardScreenshot(PathBuf::from(
                    "rewards.jpg"
                ))),
            }))
        );
        assert_eq!(
            parse_command(&arguments(&[
                "preview",
                "image",
                "relic-suggestions",
                "--era",
                "all",
                "suggestions.png",
            ])),
            Ok(Command::Preview(PreviewRequest::One {
                name: "relic-suggestions".to_owned(),
                path: PathBuf::from("suggestions.png"),
                background: None,
                source: Some(PreviewSource::RelicInventory("all".to_owned())),
            }))
        );
        assert_eq!(
            parse_command(&arguments(&[
                "preview",
                "image",
                "notification",
                "--era",
                "all",
                "notification.png",
            ])),
            Err("--era is only valid for relic-suggestions".to_owned())
        );
    }

    #[test]
    fn live_capture_is_limited_to_warframe() {
        assert_eq!(
            parse_command(&arguments(&["relic-ocr"])),
            Ok(Command::RelicOcr(None))
        );
        assert_eq!(
            parse_command(&arguments(&["screenshot", "--target", "screen"])),
            Err("unknown screenshot option: --target".to_owned())
        );
    }

    #[test]
    fn rejects_empty_launch_command() {
        assert_eq!(
            parse_command(&arguments(&["launch", "--"])),
            Err("launch requires command after --".to_owned())
        );
    }
}
