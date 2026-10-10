use std::path::PathBuf;

use clap::{Args, Subcommand};
use serde_json::json;
use wfcompanion::inspect::{
    cache,
    luau::{corpus, evidence, infer, profile::Profile, tracking},
};

use super::support::*;

#[derive(Subcommand)]
pub enum Script {
    /// List offline script profiles, or export one as editable JSON.
    Profiles { key: Option<String> },
    /// Export the feature dependency manifest as editable JSON.
    Features,
    /// Retain all installed Lua bytecode in an immutable, deduplicated snapshot.
    Snapshot {
        workspace: PathBuf,
        name: String,
        /// Warframe executable matching these cache files.
        #[arg(long)]
        executable: PathBuf,
        /// Defaults to the installed game's Cache.Windows directory.
        #[arg(long)]
        cache: Option<PathBuf>,
        /// Decode profile for this executable; unknown builds retain raw bytes.
        #[arg(long)]
        profile: Option<PathBuf>,
        /// Limit extraction to these packages; repeat to select several.
        #[arg(long)]
        package: Vec<String>,
        /// Limit resource paths by substring (records partial snapshot scope).
        #[arg(long)]
        path: Option<String>,
    },
    /// Import retained bytecode from a manifest: {profile, scripts: {resource: file}}.
    Import {
        workspace: PathBuf,
        name: String,
        manifest: PathBuf,
    },
    /// Attach immutable Packages/native evidence from a JSON binding manifest.
    Evidence {
        workspace: PathBuf,
        name: String,
        bindings: PathBuf,
    },
    /// List retained snapshots and extraction/decode coverage.
    Snapshots { workspace: PathBuf },
    /// Recheck opcode coverage and index atom hashes with example script locations.
    Coverage {
        workspace: PathBuf,
        name: String,
        #[arg(long)]
        profile: Option<PathBuf>,
    },
    /// Inspect a snapshot manifest, optionally filtering resource paths.
    Show {
        workspace: PathBuf,
        name: String,
        #[arg(long)]
        path: Option<String>,
    },
    /// Infer candidate opcode/atom correspondences from unique matching bodies.
    Infer {
        #[command(flatten)]
        pair: Pair,
        /// Write the candidate profile without overwriting an existing file.
        #[arg(long)]
        output_profile: Option<PathBuf>,
    },
    /// Compare bytecode and report feature dependencies requiring review.
    Compare {
        #[command(flatten)]
        pair: Pair,
        /// External feature dependency manifest; defaults to implemented features.
        #[arg(long)]
        features: Option<PathBuf>,
        /// Exit nonzero when a feature requires review, for update checks.
        #[arg(long)]
        check: bool,
    },
    /// Inspect raw prototypes/constants even when opcode mapping is incomplete.
    Info(Input),
    /// Write normalized bytecode for wf-luau-decompiler to stdout.
    Normalize(Input),
    /// Disassemble mapped instructions.
    Disassemble(Input),
    /// Reconstruct source using the installed wf-luau-decompiler helper.
    Decompile(Input),
    /// Write original bytecode to stdout, including from a retained snapshot.
    Read(Input),
}

#[derive(Args)]
pub struct Pair {
    workspace: PathBuf,
    before: String,
    after: String,
    /// Use an updated profile for the target without modifying the snapshot.
    #[arg(long)]
    profile: Option<PathBuf>,
}

impl Pair {
    fn load(&self) -> Result<(corpus::Snapshot, corpus::Snapshot), String> {
        let a = corpus::load(&self.workspace, &self.before)?;
        let mut b = corpus::load(&self.workspace, &self.after)?;
        if let Some(path) = &self.profile {
            let profile = Profile::load(path)?;
            profile.require_executable(&b.profile.executable_sha256)?;
            b.profile = profile;
        }
        Ok((a, b))
    }
}

#[derive(Args)]
pub struct Input {
    /// Bytecode file, or - for standard input.
    #[arg(required_unless_present_any = ["cache", "snapshot"], conflicts_with_all = ["cache", "snapshot"])]
    input: Option<PathBuf>,
    /// Read the B split directly from a cache resource.
    #[arg(long, num_args = 3, value_names = ["DIRECTORY", "PACKAGE", "RESOURCE"], conflicts_with = "snapshot")]
    cache: Vec<String>,
    /// Read retained bytecode and its profile without a game installation.
    #[arg(long, num_args = 3, value_names = ["WORKSPACE", "NAME", "RESOURCE"], conflicts_with = "adapter")]
    snapshot: Vec<String>,
    /// Built-in script profile ID or executable SHA-256; see script profiles.
    #[arg(long, conflicts_with = "profile")]
    adapter: Option<String>,
    /// External decode profile; does not enable native memory access.
    #[arg(long)]
    profile: Option<PathBuf>,
}

impl Script {
    pub fn run(self) -> Result<(), String> {
        let (action, input) = match self {
            Self::Profiles { key } => {
                return match key {
                    Some(key) => print_json(&Profile::builtin(&key)?),
                    None => print_json(
                        &Profile::builtins()
                            .iter()
                            .map(|p| {
                                json!({
                                    "id": p.id, "executable_sha256": p.executable_sha256,
                                    "status": p.status, "mapped_opcodes": p.mapping.len(),
                                })
                            })
                            .collect::<Vec<_>>(),
                    ),
                };
            }
            Self::Features => return print_json(&tracking::feature_manifest()),
            Self::Snapshot {
                workspace,
                name,
                executable,
                cache: directory,
                profile,
                package,
                path,
            } => {
                let directory = directory.map_or_else(cache::locate, Ok)?;
                let profile = profile.as_deref().map(Profile::load).transpose()?;
                let snapshot = corpus::capture(
                    corpus::CaptureOptions {
                        workspace: &workspace,
                        name: &name,
                        cache: &directory,
                        executable: &executable,
                        profile,
                        packages: package,
                        path_filter: path,
                    },
                    |path, count| {
                        if count > 0 && count % 500 == 0 {
                            eprintln!("{count} scripts retained: {path}");
                        }
                    },
                )?;
                return captured(&snapshot);
            }
            Self::Import {
                workspace,
                name,
                manifest,
            } => return captured(&corpus::import(&workspace, &name, &manifest)?),
            Self::Evidence {
                workspace,
                name,
                bindings,
            } => {
                let evidence = evidence::import(&workspace, &name, &bindings)?;
                return print_json(&json!({"snapshot": evidence.snapshot,
                    "properties": evidence.properties.len(), "native": evidence.native.len(),
                    "packages_sha256": evidence.packages_sha256}));
            }
            Self::Snapshots { workspace } => return print_json(&corpus::list(&workspace)?),
            Self::Coverage {
                workspace,
                name,
                profile,
            } => {
                let mut snapshot = corpus::load(&workspace, &name)?;
                if let Some(path) = profile {
                    let profile = Profile::load(&path)?;
                    profile.require_executable(&snapshot.profile.executable_sha256)?;
                    snapshot.profile = profile;
                }
                return print_json(&tracking::coverage(&workspace, &snapshot));
            }
            Self::Show {
                workspace,
                name,
                path,
            } => {
                let mut snapshot = corpus::load(&workspace, &name)?;
                if let Some(filter) = path {
                    snapshot.scripts.retain(|path, _| path.contains(&filter));
                }
                return print_json(&snapshot);
            }
            Self::Infer {
                pair,
                output_profile,
            } => {
                let (before, after) = pair.load()?;
                let report = infer::run(&pair.workspace, &before, &after);
                print_json(&report)?;
                if !report.conflicts.is_empty() || !report.errors.is_empty() {
                    return Err(
                        "inference has conflicts/errors; no profile written (see report)".into(),
                    );
                }
                if report.profile.mapping.is_empty() {
                    return Err("no unambiguous opcode anchors found".into());
                }
                if let Some(path) = output_profile {
                    corpus::write_new_json(&path, &report.profile)?;
                }
                return Ok(());
            }
            Self::Compare {
                pair,
                features,
                check,
            } => {
                let (before, after) = pair.load()?;
                let manifest = features
                    .as_deref()
                    .map(tracking::Manifest::load)
                    .transpose()?
                    .unwrap_or_else(tracking::Manifest::builtin);
                let report = tracking::compare(&pair.workspace, &before, &after, &manifest)?;
                print_json(&report)?;
                if check
                    && (report.features.values().any(|f| f.review_required)
                        || !report.capture_errors.is_empty()
                        || report.candidate_profiles)
                {
                    return Err("game logic requires review (see report)".into());
                }
                return Ok(());
            }
            Self::Info(input) => (ScriptAction::Info, input),
            Self::Normalize(input) => (ScriptAction::Normalize, input),
            Self::Disassemble(input) => (ScriptAction::Disassemble, input),
            Self::Decompile(input) => (ScriptAction::Decompile, input),
            Self::Read(input) => (ScriptAction::Read, input),
        };
        let source = match (input.cache.as_slice(), input.snapshot.as_slice()) {
            ([directory, package, resource], _) => ScriptInput::Cache {
                directory: directory.into(),
                package: package.clone(),
                resource: resource.clone(),
            },
            (_, [workspace, name, resource]) => ScriptInput::Snapshot {
                workspace: workspace.into(),
                name: name.clone(),
                resource: resource.clone(),
            },
            _ => match input.input {
                Some(path) if path.as_os_str() == "-" => ScriptInput::Stdin,
                Some(path) => ScriptInput::File(path),
                None => return Err("script input is required".into()),
            },
        };
        script(action, source, input.adapter, input.profile)
    }
}

fn captured(snapshot: &corpus::Snapshot) -> Result<(), String> {
    print_json(&corpus::summary(snapshot))?;
    if !snapshot.errors.is_empty() {
        return Err("snapshot retained with extraction errors (see report)".into());
    }
    Ok(())
}
