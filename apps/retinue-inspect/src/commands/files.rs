use super::{args, support::*};
use clap::{Args, Subcommand};
use retinue_warframe::inspect;
use serde_json::json;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum Cache {
    /// Show configured Oodle decoder availability.
    Decoder,
    /// Locate Warframe's Cache.Windows using Steam's library metadata.
    Locate,
    /// List package names in a Cache.Windows directory.
    Packages { directory: Option<PathBuf> },
    /// List resource paths and split sizes.
    Paths {
        directory: PathBuf,
        package: String,
        substring: Option<String>,
    },
    /// Extract one resource to OUTPUT.h.raw, OUTPUT.b.raw, and OUTPUT.f.raw.
    Extract {
        directory: PathBuf,
        package: String,
        resource: String,
        output: PathBuf,
    },
    /// Write one decompressed split to stdout (raw bytes).
    Read {
        directory: PathBuf,
        package: String,
        resource: String,
        #[arg(value_parser = args::split, default_value = "B")]
        split: char,
    },
    /// Find literal bytes, retaining offsets and per-resource failures.
    Find {
        directory: PathBuf,
        package: String,
        text: String,
        #[command(flatten)]
        options: Search,
    },
    /// Find a hexadecimal byte sequence.
    Scan {
        directory: PathBuf,
        package: String,
        #[arg(value_parser = args::hex)]
        pattern: args::Hex,
        #[command(flatten)]
        options: Search,
    },
}

#[derive(Args)]
pub struct Search {
    #[arg(long, value_parser = args::split)]
    split: Option<char>,
    /// Restrict resource paths by substring.
    #[arg(long)]
    path: Option<String>,
    /// Continue after resource errors; output remains marked incomplete.
    #[arg(long)]
    continue_on_error: bool,
    #[arg(long, default_value_t = 10000)]
    limit: usize,
    #[arg(long, default_value_t = 67108864)]
    max_resource_bytes: usize,
    /// Emit each match/error immediately as NDJSON, then a summary.
    #[arg(long)]
    stream: bool,
}

impl Cache {
    pub fn run(self) -> Result<(), String> {
        match self {
            Self::Decoder => print_json(&inspect::cache::oodle_decoder_status()),
            Self::Locate => print_json(&json!({"directory": inspect::cache::locate()?})),
            Self::Packages { directory } => {
                let directory = match directory {
                    Some(directory) => directory,
                    None => inspect::cache::locate()?,
                };
                print_json(
                    &json!({"directory": directory, "packages": inspect::cache::packages(&directory)?}),
                )
            }
            Self::Paths {
                directory,
                package,
                substring,
            } => cache_paths(directory, package, substring),
            Self::Extract {
                directory,
                package,
                resource,
                output,
            } => cache_extract(directory, package, resource, output),
            Self::Read {
                directory,
                package,
                resource,
                split,
            } => write_bytes(
                &inspect::cache::read_resource_split(&directory, &package, &resource, split)?.data,
            ),
            Self::Find {
                directory,
                package,
                text,
                options,
            } => options.run(directory, package, text.as_bytes()),
            Self::Scan {
                directory,
                package,
                pattern,
                options,
            } => options.run(directory, package, &pattern.0),
        }
    }
}

impl Search {
    fn run(self, directory: PathBuf, package: String, needle: &[u8]) -> Result<(), String> {
        let options = inspect::cache::SearchOptions {
            split: self.split,
            path: self.path,
            continue_on_error: self.continue_on_error,
            max_matches: self.limit,
            max_resource_bytes: self.max_resource_bytes,
        };
        let mut records = Vec::new();
        if self.stream {
            print_ndjson(
                "game.cache",
                "start",
                &json!({"directory": directory, "package": package, "options": options}),
            )?;
        }
        let summary = inspect::cache::search(&directory, &package, needle, &options, |record| {
            if self.stream {
                print_ndjson("game.cache", "result", record)
            } else {
                records.push(serde_json::to_value(record).map_err(|e| e.to_string())?);
                Ok(())
            }
        })?;
        if self.stream {
            print_ndjson("game.cache", "summary", &summary)?;
        } else {
            print_json(
                &json!({"directory": directory, "package": package, "results": records, "summary": summary}),
            )?;
        }
        if summary.errors > 0 {
            return Err("cache search incomplete: resource errors (see results)".into());
        }
        Ok(())
    }
}

#[derive(Subcommand)]
pub enum Report {
    /// Validate report schema and executable identity.
    Verify(ReportFile),
    /// List report queries.
    List(ReportFile),
    /// Return one query's complete payload.
    Get {
        #[command(flatten)]
        file: ReportFile,
        index: usize,
    },
}

#[derive(Args)]
pub struct ReportFile {
    file: PathBuf,
    /// Expected executable SHA-256; Warframe scope also accepts adapter keys.
    #[arg(long = "sha256", alias = "adapter")]
    adapter: Option<String>,
}

impl Report {
    pub fn run(self, warframe: bool) -> Result<(), String> {
        if !warframe {
            use retinue_native::inspect::report;
            return match self {
                Self::Verify(file) => {
                    print_json(&report::verify(&file.file, file.adapter.as_deref())?)
                }
                Self::List(file) => print_json(&report::list(&file.file, file.adapter.as_deref())?),
                Self::Get { file, index } => {
                    print_json(&report::get(&file.file, index, file.adapter.as_deref())?)
                }
            };
        }
        match self {
            Self::Verify(file) => print_json(&inspect::report::verify(
                &file.file,
                file.adapter.as_deref(),
            )?),
            Self::List(file) => {
                print_json(&inspect::report::list(&file.file, file.adapter.as_deref())?)
            }
            Self::Get { file, index } => print_json(&inspect::report::get(
                &file.file,
                index,
                file.adapter.as_deref(),
            )?),
        }
    }
}
