//! The `cutaway` CLI: every pipeline step headless, through the same service as the app.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};

use apc_app::Session;

#[derive(Parser, Debug)]
#[command(
    name = "cutaway",
    version,
    about = "Script-matched rough cuts from unslated audio play sessions"
)]
pub struct Cli {
    /// Episode file or folder (default: $CUTAWAY_PROJECT or the episode in the current folder).
    #[arg(long, short = 'p', global = true)]
    pub project: Option<PathBuf>,
    /// Confirm destructive actions and overwrites.
    #[arg(long, short = 'y', global = true)]
    pub yes: bool,
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Show commands.
    Show {
        #[command(subcommand)]
        cmd: ShowCmd,
    },
    /// Create an episode (inside a show with --show).
    New {
        name: String,
        #[arg(long)]
        show: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        parent: PathBuf,
    },
    /// Add a new script version.
    ImportScript {
        file: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Take only the story context (character bios, settings) from a Throughline .thrl file.
    ImportStory {
        file: PathBuf,
    },
    /// Import audio (files or folders) and assign roles.
    ImportAudio {
        paths: Vec<PathBuf>,
        /// Copy into the project (default).
        #[arg(long, conflicts_with = "reference")]
        copy: bool,
        /// Reference the files in place.
        #[arg(long)]
        reference: bool,
    },
    /// Show the file-to-role mapping; --set SOURCE=ROLE[:CHARACTER] corrects it.
    Roles {
        #[arg(long = "set")]
        set: Vec<String>,
    },
    /// Run the local ASR (or import transcript sidecars).
    Transcribe,
    /// Deterministic matching + AI for ambiguous parts (the full analysis). With
    /// --character or --line, only those; the rest of the episode stays as it is.
    Align {
        /// Re-analyse only this character's session (repeatable).
        #[arg(long = "character")]
        characters: Vec<String>,
        /// Re-analyse only this line's takes (repeatable).
        #[arg(long = "line")]
        lines: Vec<String>,
    },
    /// Room classification, note condensing, picks.
    Notes,
    /// Sound effects.
    Sfx {
        #[command(subcommand)]
        cmd: SfxCmd,
    },
    /// Export (fcpxml, srt, cutlist, mix).
    Export {
        format: String,
        #[arg(long)]
        scene: Option<String>,
        #[arg(long)]
        out: Option<PathBuf>,
        /// cutlist: csv or md; mix: wav or mp3.
        #[arg(long)]
        kind: Option<String>,
    },
    /// Progress, match rate, flagged items.
    Status,
    /// The last match report.
    Report,
    Undo,
    Redo,
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Backups and named versions (save NAME, revert ID).
    Versions {
        action: Option<String>,
        arg: Option<String>,
    },
    /// Set a project setting (e.g. ai.model claude-sonnet-5-5).
    Config {
        key: String,
        value: String,
    },
    /// Connect a sound service (epidemic, elevenlabs): the API key goes into the OS keychain.
    /// The key is read from a hidden prompt, or from stdin when piped.
    Connect {
        service: String,
    },
    /// Remove a sound service's API key from the keychain.
    Disconnect {
        service: String,
    },
    /// Which sound services are connected.
    Connections,
    /// Call any service method with JSON params.
    Call {
        method: String,
        params: Option<String>,
    },
    /// List the service methods.
    Methods,
    /// Run as an MCP server on stdio.
    Mcp,
    /// Create the synthetic demo episode in DIR.
    Demo {
        dir: PathBuf,
        #[arg(long)]
        analyze: bool,
    },
    /// HTTP bridge for the browser UI and UI tests.
    Serve {
        #[arg(long, default_value = "127.0.0.1:7357")]
        addr: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ShowCmd {
    /// Create a show folder with a library.
    New {
        name: String,
        #[arg(long, default_value = ".")]
        parent: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub enum SfxCmd {
    /// AI sound spotting into the bucket.
    Spot {
        #[arg(long)]
        scene: String,
    },
    /// Spot script into buckets: new and changed scenes (one AI call each), or all.
    SpotScript {
        #[arg(long)]
        scene: Option<String>,
        /// Re-spot every scene, not only new and changed ones.
        #[arg(long)]
        all: bool,
    },
    /// Search Epidemic Sound (nothing is downloaded).
    Search {
        query: String,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Add files to the show library.
    Import { paths: Vec<PathBuf> },
}

/// Reads a secret without echo (or the first line of stdin when it isn't a terminal).
fn read_secret(prompt: &str) -> anyhow::Result<String> {
    use std::io::IsTerminal;
    let key = if std::io::stdin().is_terminal() {
        rpassword::prompt_password(prompt)?
    } else {
        let mut s = String::new();
        std::io::stdin().read_line(&mut s)?;
        s
    };
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(anyhow!("no key given"));
    }
    Ok(key)
}

fn find_project(p: &Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(p) = p {
        return Ok(p.clone());
    }
    if let Ok(e) = std::env::var("CUTAWAY_PROJECT") {
        return Ok(PathBuf::from(e));
    }
    let cwd = std::env::current_dir()?;
    apc_core::project::find_episode_file(&cwd)
        .map_err(|_| anyhow!("no episode here; pass --project or set CUTAWAY_PROJECT"))
}

fn open(cli: &Cli) -> anyhow::Result<Session> {
    let p = find_project(&cli.project)?;
    Session::with_project(&p, "cli").with_context(|| format!("opening {}", p.display()))
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn call(sess: &mut Session, m: &str, params: Value) -> anyhow::Result<Value> {
    sess.call(m, &params).map_err(|e| anyhow!("{e}"))
}

/// Runs one CLI invocation. Returns the JSON result (printed by `main`).
pub fn run(cli: Cli) -> anyhow::Result<Value> {
    let confirm = cli.yes;
    match &cli.cmd {
        Cmd::Show {
            cmd: ShowCmd::New { name, parent },
        } => {
            let mut sess = Session::new("cli");
            call(
                &mut sess,
                "create_show",
                json!({ "parent": s(parent), "name": name }),
            )
        }
        Cmd::New { name, show, parent } => {
            let mut sess = Session::new("cli");
            call(
                &mut sess,
                "create_episode",
                json!({ "parent": s(parent), "name": name, "show": show.as_ref().map(|p| s(p)) }),
            )
        }
        Cmd::Demo { dir, analyze } => {
            std::fs::create_dir_all(dir)?;
            let svc = apc_app::demo::create(dir, "cli", *analyze).map_err(|e| anyhow!("{e}"))?;
            Ok(json!({ "file": s(&svc.paths.file) }))
        }
        Cmd::Methods => Ok(json!(apc_app::methods()
            .iter()
            .map(|m| json!({ "name": m.name, "description": m.description, "mcp": m.mcp }))
            .collect::<Vec<_>>())),
        Cmd::Mcp => {
            let sess = match find_project(&cli.project) {
                Ok(p) => Session::with_project(&p, "mcp").map_err(|e| anyhow!("{e}"))?,
                Err(_) => Session::new("mcp"),
            };
            apc_mcp::run_stdio(sess)?;
            Ok(Value::Null)
        }
        Cmd::Serve { addr } => {
            let sess = match find_project(&cli.project) {
                Ok(p) => Session::with_project(&p, "ui").map_err(|e| anyhow!("{e}"))?,
                Err(_) => Session::new("ui"),
            };
            apc_app::bridge::serve(addr, sess).map_err(|e| anyhow!("{e}"))?;
            Ok(Value::Null)
        }
        _ => {
            let mut sess = open(&cli)?;
            match &cli.cmd {
                Cmd::ImportScript { file, name } => call(
                    &mut sess,
                    "import_script",
                    json!({ "path": s(file), "name": name }),
                ),
                Cmd::ImportStory { file } => {
                    call(&mut sess, "import_story", json!({ "path": s(file) }))
                }
                Cmd::ImportAudio {
                    paths, reference, ..
                } => call(
                    &mut sess,
                    "import_audio",
                    json!({ "paths": paths.iter().map(|p| s(p)).collect::<Vec<_>>(), "copy": !reference }),
                ),
                Cmd::Roles { set } if !set.is_empty() => {
                    let mut items = Vec::new();
                    for x in set {
                        let (sid, rest) = x
                            .split_once('=')
                            .ok_or_else(|| anyhow!("use SOURCE=ROLE[:CHARACTER]"))?;
                        let (role, ch) = match rest.split_once(':') {
                            Some((r, c)) => (r, Some(c.to_string())),
                            None => (rest, None),
                        };
                        items.push(json!({ "source_id": sid, "role": role, "character": ch }));
                    }
                    call(&mut sess, "confirm_roles", json!({ "assignments": items }))
                }
                Cmd::Roles { .. } => call(&mut sess, "roles", json!({})),
                Cmd::Transcribe => call(&mut sess, "run_transcription", json!({})),
                Cmd::Align { characters, lines } if characters.is_empty() && lines.is_empty() => {
                    call(&mut sess, "run_alignment", json!({}))
                }
                Cmd::Align { characters, lines } => call(
                    &mut sess,
                    "analyze_scope",
                    json!({ "characters": characters, "line_ids": lines }),
                ),
                Cmd::Notes => call(&mut sess, "run_notes", json!({})),
                Cmd::Sfx {
                    cmd: SfxCmd::Spot { scene },
                } => call(&mut sess, "spot_sound_effects", json!({ "scene": scene })),
                Cmd::Sfx {
                    cmd: SfxCmd::SpotScript { scene, all },
                } => call(
                    &mut sess,
                    "spot_buckets",
                    json!({ "scene": scene, "all": all }),
                ),
                Cmd::Sfx {
                    cmd: SfxCmd::Search { query, offset },
                } => call(
                    &mut sess,
                    "search_epidemic",
                    json!({ "query": query, "offset": offset }),
                ),
                Cmd::Sfx {
                    cmd: SfxCmd::Import { paths },
                } => call(
                    &mut sess,
                    "import_sounds",
                    json!({ "paths": paths.iter().map(|p| s(p)).collect::<Vec<_>>() }),
                ),
                Cmd::Export {
                    format,
                    scene,
                    out,
                    kind,
                } => {
                    let m = match format.as_str() {
                        "fcpxml" => "export_fcpxml",
                        "srt" => "export_srt",
                        "cutlist" | "cut-list" => "export_cut_list",
                        "mix" | "wav" | "mp3" => "export_mix",
                        other => {
                            return Err(anyhow!(
                                "unknown export format '{other}' (fcpxml, srt, cutlist, mix)"
                            ))
                        }
                    };
                    let mut p = json!({ "scene": scene, "path": out.as_ref().map(|o| s(o)), "confirm": confirm, "format": kind });
                    if m == "export_mix" {
                        let scene = scene.clone().ok_or_else(|| anyhow!("mix needs --scene"))?;
                        let fmt = kind.clone().unwrap_or_else(|| {
                            if format == "mp3" {
                                "mp3".into()
                            } else {
                                "wav".into()
                            }
                        });
                        p = json!({ "scene": scene, "format": fmt, "confirm": confirm });
                    }
                    call(&mut sess, m, p)
                }
                Cmd::Connect { service } => {
                    let key = read_secret(&format!("{service} API key (input hidden): "))?;
                    call(
                        &mut sess,
                        "connect_service",
                        json!({ "service": service, "api_key": key }),
                    )
                }
                Cmd::Disconnect { service } => call(
                    &mut sess,
                    "disconnect_service",
                    json!({ "service": service }),
                ),
                Cmd::Connections => call(&mut sess, "connections", json!({})),
                Cmd::Status => call(&mut sess, "project_status", json!({})),
                Cmd::Report => call(&mut sess, "match_report", json!({})),
                Cmd::Undo => call(&mut sess, "undo", json!({})),
                Cmd::Redo => call(&mut sess, "redo", json!({})),
                Cmd::History { limit } => call(&mut sess, "history", json!({ "limit": limit })),
                Cmd::Versions { action, arg } => match action.as_deref() {
                    None | Some("list") => call(&mut sess, "list_versions", json!({})),
                    Some("save") => call(
                        &mut sess,
                        "save_version",
                        json!({ "name": arg.clone().unwrap_or_default() }),
                    ),
                    Some("revert") => call(
                        &mut sess,
                        "revert",
                        json!({ "backup_id": arg.clone().unwrap_or_default(), "confirm": confirm }),
                    ),
                    Some(o) => Err(anyhow!(
                        "unknown versions action '{o}' (list, save, revert)"
                    )),
                },
                Cmd::Config { key, value } => {
                    let v: Value =
                        serde_json::from_str(value).unwrap_or(Value::String(value.clone()));
                    call(&mut sess, "set_config", json!({ "key": key, "value": v }))
                }
                Cmd::Call { method, params } => {
                    let p: Value = match params {
                        Some(t) => serde_json::from_str(t).context("params must be JSON")?,
                        None => json!({}),
                    };
                    call(&mut sess, method, p)
                }
                _ => unreachable!("handled above"),
            }
        }
    }
}
