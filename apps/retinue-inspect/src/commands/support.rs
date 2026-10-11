use retinue_warframe::game_observer;
use retinue_warframe::inspect::luau::{corpus, profile::Profile};
use serde::Serialize;
use serde_json::json;
use std::io::{self, Read, Write};
use std::path::PathBuf;
pub(super) const BROKEN_PIPE: &str = "__retinue_inspect_broken_pipe__";

pub(super) enum ScriptAction {
    Read,
    Info,
    Normalize,
    Disassemble,
    Decompile,
}
pub(super) enum ScriptInput {
    Stdin,
    File(PathBuf),
    Snapshot {
        workspace: PathBuf,
        name: String,
        resource: String,
    },
    Cache {
        directory: PathBuf,
        package: String,
        resource: String,
    },
}

pub(super) fn doctor() -> Result<(), String> {
    let game = match game_observer::current_process_identity() {
        Ok(identity) => json!({
            "status": if identity.is_some() { "running" } else { "stopped" },
            "adapter": game_observer::adapter::support(identity.as_ref()),
            "identity": identity,
        }),
        Err(error) => json!({"status": "error", "error": error}),
    };
    print_json(&json!({
        "retinue-inspect": {"version": env!("CARGO_PKG_VERSION")},
        "tools": {
            "luau_decompiler": retinue_warframe::inspect::luau::decompiler_status(),
            "oodle_decoder": retinue_warframe::inspect::cache::oodle_decoder_status(),
        },
        "game": game,
    }))
}

pub(super) fn script(
    action: ScriptAction,
    input: ScriptInput,
    adapter: Option<String>,
    profile_path: Option<PathBuf>,
) -> Result<(), String> {
    let (bytecode, source, stored_profile) = read_script(input)?;
    if matches!(action, ScriptAction::Read) {
        return write_bytes(&bytecode);
    }
    let adapter = match profile_path {
        Some(path) => {
            let profile = Profile::load(&path)?;
            if let Some(stored) = stored_profile {
                profile.require_executable(&stored.executable_sha256)?;
            }
            profile
        }
        None if stored_profile.is_some() => stored_profile.unwrap(),
        None => {
            let key = match adapter {
                Some(key) => key,
                None => game_observer::current_process_identity()?
                    .map(|identity| identity.executable.sha256)
                    .ok_or_else(|| {
                        "--adapter or --profile is required when Warframe is not running".to_owned()
                    })?,
            };
            retinue_warframe::inspect::luau::profile::Profile::builtin(&key)?
        }
    };
    match action {
        ScriptAction::Read => unreachable!(),
        ScriptAction::Info => print_json(&json!({
            "source": source,
            "script": retinue_warframe::inspect::luau::info(&bytecode, &adapter)?,
        })),
        ScriptAction::Normalize => write_bytes(&retinue_warframe::inspect::luau::normalize(
            &bytecode, &adapter,
        )?),
        ScriptAction::Disassemble => write_stdout(&retinue_warframe::inspect::luau::disassemble(
            &bytecode, &adapter,
        )?),
        ScriptAction::Decompile => write_stdout(&retinue_warframe::inspect::luau::decompile(
            &bytecode, &adapter,
        )?),
    }
}

pub(super) fn write_bytes(bytes: &[u8]) -> Result<(), String> {
    match io::stdout().lock().write_all(bytes) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Err(BROKEN_PIPE.to_owned()),
        Err(error) => Err(format!("could not write standard output: {error}")),
    }
}

type ScriptSource = (Vec<u8>, serde_json::Value, Option<Profile>);

pub(super) fn read_script(input: ScriptInput) -> Result<ScriptSource, String> {
    match input {
        ScriptInput::Stdin => {
            let mut bytes = Vec::new();
            io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|error| format!("could not read standard input: {error}"))?;
            Ok((bytes, json!({"kind": "stdin"}), None))
        }
        ScriptInput::File(path) => {
            let bytes = std::fs::read(&path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;
            Ok((bytes, json!({"kind": "file", "path": path}), None))
        }
        ScriptInput::Snapshot {
            workspace,
            name,
            resource,
        } => {
            let snapshot = corpus::load(&workspace, &name)?;
            let bytes = corpus::read_script(&workspace, &snapshot, &resource)?;
            Ok((
                bytes,
                json!({"kind": "snapshot", "workspace": workspace, "name": name, "resource": resource}),
                Some(snapshot.profile),
            ))
        }
        ScriptInput::Cache {
            directory,
            package,
            resource,
        } => {
            let body = retinue_warframe::inspect::cache::read_resource_split(
                &directory, &package, &resource, 'B',
            )?;
            Ok((
                body.data,
                json!({
                    "kind": "cache",
                    "directory": directory,
                    "package": package,
                    "path": body.path,
                    "split": body.split,
                }),
                None,
            ))
        }
    }
}

pub(super) fn game_pid() -> Result<u32, String> {
    game_observer::find_warframe()
        .pid()
        .ok_or_else(|| "Warframe is not running".to_owned())
}

pub(super) fn process() -> Result<(), String> {
    let state = game_observer::find_warframe();
    let identity = game_observer::current_process_identity()?;
    print_json(&json!({"game": state, "identity": identity}))
}

pub(super) fn adapter() -> Result<(), String> {
    let pid = game_pid()?;
    let identity = game_observer::identify_process(pid)?;
    let scaleform = game_observer::ui::bounded_probe_for_identity(pid, &identity);
    let adapter = match &scaleform {
        game_observer::ui::BoundedProbe::Available { .. } => {
            game_observer::adapter::AdapterSupport::Supported {
                id: "validated_bindings",
                capabilities: &["scaleform_ui_v1"],
            }
        }
        game_observer::ui::BoundedProbe::Unavailable { reason, .. } => {
            game_observer::adapter::AdapterSupport::Unsupported {
                reason: reason.clone(),
            }
        }
    };
    let metadata = game_observer::metadata::capture_for_identity(pid, identity.clone())
        .map(|value| json!({"status": "available", "value": value}))
        .unwrap_or_else(|reason| json!({"status": "unavailable", "reason": reason}));
    let inventory = game_observer::inventory::Reader::open_for_identity(&identity)
        .and_then(|mut reader| reader.read())
        .map(|snapshot| {
            json!({"status": "available", "sync": snapshot.sync,
            "collections": snapshot.fields.iter().map(|(key, value)|
                (key.clone(), json!(value.as_array().map(Vec::len))))
                .collect::<serde_json::Map<_, _>>()})
        })
        .unwrap_or_else(|reason| json!({"status": "unavailable", "reason": reason}));
    print_json(&json!({
        "identity": identity,
        "adapter": adapter,
        "probes": {
            "game_metadata": metadata,
            "inventory": inventory,
            "scaleform": scaleform,
        },
    }))
}

pub(super) fn metadata(output: Option<PathBuf>) -> Result<(), String> {
    let captured = game_observer::metadata::capture(game_pid()?)?;
    let rendered = serde_json::to_string_pretty(&captured)
        .map_err(|error| format!("could not encode report: {error}"))?;
    if let Some(path) = output {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        std::fs::write(&path, format!("{rendered}\n"))
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    } else {
        write_stdout(&format!("{rendered}\n"))?;
    }
    Ok(())
}

pub(super) fn cache_paths(
    cache_dir: PathBuf,
    package: String,
    substring: Option<String>,
) -> Result<(), String> {
    let mut paths = retinue_warframe::inspect::cache::paths(&cache_dir, &package)?;
    if let Some(substring) = substring {
        paths.retain(|entry| entry.path.contains(&substring));
    }
    print_json(&json!({
        "cache_dir": cache_dir,
        "package": package,
        "paths": paths,
    }))
}

pub(super) fn cache_extract(
    cache_dir: PathBuf,
    package: String,
    resource: String,
    output: PathBuf,
) -> Result<(), String> {
    let extracted =
        retinue_warframe::inspect::cache::extract(&cache_dir, &package, &resource, &output)?;
    print_json(&json!({
        "cache_dir": cache_dir,
        "package": package,
        "resources": extracted,
    }))
}

pub(super) fn print_json(value: &impl serde::Serialize) -> Result<(), String> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|error| format!("could not encode report: {error}"))?;
    write_stdout(&format!("{rendered}\n"))
}

pub(super) fn print_ndjson<T: Serialize>(
    stream: &'static str,
    record: &'static str,
    data: &T,
) -> Result<(), String> {
    #[derive(Serialize)]
    struct Envelope<'a, T> {
        schema: &'static str,
        schema_version: u8,
        emitted_at_unix_ms: u128,
        stream: &'static str,
        record: &'static str,
        data: &'a T,
    }

    let mut line = serde_json::to_vec(&Envelope {
        schema: "wfinspect.stream",
        schema_version: 1,
        emitted_at_unix_ms: retinue_native::inspect::unix_time_ms(),
        stream,
        record,
        data,
    })
    .map_err(|error| format!("could not encode stream record: {error}"))?;
    line.push(b'\n');
    let mut output = io::stdout().lock();
    match output.write_all(&line).and_then(|()| output.flush()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Err(BROKEN_PIPE.to_owned()),
        Err(error) => Err(format!("could not write output: {error}")),
    }
}

pub(super) fn write_stdout(text: &str) -> Result<(), String> {
    match io::stdout().lock().write_all(text.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(format!("could not write output: {error}")),
    }
}
