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

/// Who's writing, when it isn't the reviewer this computer remembers.
#[derive(clap::Args, Debug)]
pub struct AuthorArgs {
    /// A name.
    #[arg(long = "as")]
    pub author: Option<String>,
    /// director, producer, writer, actor, client, editor or other.
    #[arg(long, default_value = "other")]
    pub role: String,
}

impl AuthorArgs {
    fn json(&self) -> Value {
        match &self.author {
            Some(name) => json!({ "name": name, "role": self.role }),
            None => Value::Null,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum NoteCmd {
    /// Write a note. --at and --to hang it on what's said there, --line and --sound on that
    /// element, and with none of them it's about the scene.
    Add {
        text: String,
        #[arg(long)]
        scene: Option<String>,
        /// Scene seconds.
        #[arg(long)]
        at: Option<f64>,
        #[arg(long)]
        to: Option<f64>,
        #[arg(long)]
        line: Option<String>,
        /// A word of the line, numbered from 0.
        #[arg(long)]
        word: Option<usize>,
        #[arg(long)]
        to_word: Option<usize>,
        #[arg(long)]
        sound: Option<String>,
        /// performance, timing, sound, music or script.
        #[arg(long)]
        tag: Option<String>,
        #[command(flatten)]
        author: AuthorArgs,
    },
    /// The one feedback set, in threads.
    List {
        #[arg(long)]
        scene: Option<String>,
        /// open or resolved.
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        author: Option<String>,
        #[arg(long)]
        round: Option<String>,
    },
    Reply {
        note: String,
        text: String,
        #[command(flatten)]
        author: AuthorArgs,
    },
    Edit {
        note: String,
        text: String,
        #[command(flatten)]
        author: AuthorArgs,
    },
    Tag {
        note: String,
        tag: Option<String>,
        #[command(flatten)]
        author: AuthorArgs,
    },
    /// Hang a note on another spot: --at and --to, --line, --sound, or just --scene.
    Move {
        note: String,
        #[arg(long)]
        scene: Option<String>,
        #[arg(long)]
        at: Option<f64>,
        #[arg(long)]
        to: Option<f64>,
        #[arg(long)]
        line: Option<String>,
        #[arg(long)]
        word: Option<usize>,
        #[arg(long)]
        to_word: Option<usize>,
        #[arg(long)]
        sound: Option<String>,
        #[command(flatten)]
        author: AuthorArgs,
    },
    /// Remove a note and its replies (needs --yes). Undo brings it back.
    Remove {
        note: String,
        #[command(flatten)]
        author: AuthorArgs,
    },
    Resolve {
        note: String,
        #[command(flatten)]
        author: AuthorArgs,
    },
    Reopen {
        note: String,
        #[command(flatten)]
        author: AuthorArgs,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReviewCmd {
    /// Open scenes for review: keeps which cut they're in and marks them Review.
    Start {
        #[arg(required = true)]
        scenes: Vec<String>,
    },
    /// Close a round. It stays in the history.
    Close { round: String },
    /// Who's reviewing here, or say so with a name.
    Who {
        name: Option<String>,
        #[arg(long, default_value = "other")]
        role: String,
        /// Keep this person on this computer.
        #[arg(long)]
        remember: bool,
    },
    /// Rounds, arrivals, and the counts per scene and reviewer.
    History,
    /// Search scenes, lines and notes across every round.
    Search { query: String },
    /// Merge what's new in the shared folder and write this machine's own file.
    Sync,
    /// Add a review file: merged, then archived in Reviews/Imported.
    Add {
        file: PathBuf,
        /// Even if it belongs to another project.
        #[arg(long)]
        anyway: bool,
    },
    /// Save the current reviewer's notes as one review file.
    Send { path: PathBuf },
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Show commands.
    Show {
        #[command(subcommand)]
        cmd: ShowCmd,
    },
    /// What the welcome screen shows for these episodes and show folders.
    Shelf {
        paths: Vec<PathBuf>,
    },
    /// Set the cover of a show or an episode (stored as Cover.jpg in its folder).
    Cover {
        target: PathBuf,
        image: PathBuf,
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
    /// Export as on the Export page: versioned folders, never overwriting, scenes locked after.
    /// Whole episode without --scene. With --episode, a show export to its pipeline folder.
    Deliver(DeliverArgs),
    /// Lock a scene's timing.
    Lock {
        scene: String,
    },
    /// Unlock a scene (needs --yes): the exported files won't match it anymore.
    Unlock {
        scene: String,
    },
    /// Run a line side by side with another. Without one, it goes back in the chain.
    Beside {
        line: String,
        with: Option<String>,
        /// Seconds after that line's start, negative to start first.
        #[arg(long, allow_negative_numbers = true)]
        offset: Option<f64>,
    },
    /// Move lines back in the room: 0 is where the character stands, 1 is far.
    Depth {
        depth: f64,
        #[arg(required = true)]
        lines: Vec<String>,
    },
    /// Mark how far along a scene is: in_progress, review or finished. Without one, clears it.
    Mark {
        scene: String,
        status: Option<String>,
    },
    /// Review notes: write them, answer them, settle them.
    Note {
        #[command(subcommand)]
        cmd: NoteCmd,
    },
    /// Review rounds, who's reviewing, and the review files.
    Review {
        #[command(subcommand)]
        cmd: ReviewCmd,
    },
    /// Every scene's lock and export history.
    Exports,
    /// The show's pipeline folder, or a proposal; --set with --yes confirms one.
    Pipeline {
        #[arg(long)]
        set: Option<PathBuf>,
    },
    /// Save the settings of `deliver` as a preset, or remove one with --delete.
    Preset {
        name: String,
        #[arg(long)]
        delete: bool,
        #[command(flatten)]
        settings: DeliverSettings,
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

/// `deliver`: what to export, and how.
#[derive(clap::Args, Debug)]
pub struct DeliverArgs {
    /// A scene to export (repeatable); the whole episode without it.
    #[arg(long = "scene")]
    scenes: Vec<String>,
    /// Only the scenes marked so (repeatable): in-progress, review or finished.
    #[arg(long = "status")]
    status: Vec<String>,
    /// Show export: an episode folder of the show (repeatable), to its pipeline folder.
    #[arg(long = "episode")]
    episodes: Vec<String>,
    /// Use a saved preset instead of the options below.
    #[arg(long)]
    preset: Option<String>,
    /// Only say what would be written.
    #[arg(long)]
    dry_run: bool,
    #[command(flatten)]
    settings: DeliverSettings,
}

#[derive(clap::Args, Debug)]
pub struct DeliverSettings {
    /// stems, clips, mix, one-file (the range as one mix) or fcpxml.
    #[arg(long = "as", default_value = "stems")]
    mode: String,
    /// Only these tracks, comma separated: dialogue, room-voices, effects, room-effects,
    /// ambience, music, mix.
    #[arg(long, value_delimiter = ',')]
    tracks: Vec<String>,
    /// One full-length file per character instead of one dialogue file.
    #[arg(long)]
    split_dialogue: bool,
    /// wav, flac, or mp3 (mix and one-file only).
    #[arg(long, default_value = "wav")]
    format: String,
    /// Sample rate.
    #[arg(long, default_value_t = 48000)]
    rate: u32,
    /// 16, 24 or 32 (float, WAV only).
    #[arg(long, default_value_t = 24)]
    bits: u16,
    /// The mix's target in LUFS, or `off`. Stems never change.
    #[arg(long, default_value = "-16", allow_hyphen_values = true)]
    loudness: String,
    #[arg(long)]
    no_fcpxml: bool,
    /// What the FCPXML's clips point at: original or stems.
    #[arg(long, default_value = "original")]
    fcpxml_source: String,
    #[arg(long)]
    no_subtitles: bool,
    /// Also a cut list: csv or md.
    #[arg(long)]
    cut_list: Option<String>,
    /// Leave the scenes unlocked.
    #[arg(long)]
    no_lock: bool,
    /// episode or pipeline.
    #[arg(long, default_value = "episode")]
    to: String,
}

impl DeliverSettings {
    /// The settings as the service reads them, which refuses what can't be delivered.
    fn to_json(&self) -> anyhow::Result<Value> {
        let loudness = match self.loudness.as_str() {
            "off" => Value::Null,
            l => json!(l
                .parse::<f64>()
                .map_err(|_| anyhow!("--loudness is a number of LUFS, or off"))?),
        };
        let all = self.tracks.is_empty();
        let on = |name: &str| all || self.tracks.iter().any(|t| t == name);
        for t in &self.tracks {
            let known = [
                "dialogue",
                "room-voices",
                "effects",
                "room-effects",
                "ambience",
                "music",
                "mix",
            ];
            if !known.contains(&t.as_str()) {
                return Err(anyhow!("unknown track '{t}' ({})", known.join(", ")));
            }
        }
        Ok(json!({
            "mode": snake(&self.mode),
            "tracks": {
                "dialogue": on("dialogue"), "room_voices": on("room-voices"),
                "effects": on("effects"), "room_effects": on("room-effects"),
                "ambience": on("ambience"), "music": on("music"), "mix": on("mix"),
                "split_dialogue": self.split_dialogue,
            },
            "codec": self.format,
            "sample_rate": self.rate,
            "bit_depth": self.bits,
            "loudness": loudness,
            "fcpxml": !self.no_fcpxml,
            "fcpxml_source": self.fcpxml_source,
            "subtitles": !self.no_subtitles,
            "cut_list": self.cut_list,
            "lock_after": !self.no_lock,
            "save_to": self.to,
        }))
    }
}

/// `deliver`: the page's one button.
fn deliver(sess: &mut Session, a: &DeliverArgs) -> anyhow::Result<Value> {
    let (settings, preset) = match &a.preset {
        Some(p) => (Value::Null, json!(p)),
        None => (a.settings.to_json()?, Value::Null),
    };
    let status =
        (!a.status.is_empty()).then(|| a.status.iter().map(|s| snake(s)).collect::<Vec<_>>());
    if !a.episodes.is_empty() {
        if a.dry_run || status.is_some() {
            return Err(anyhow!(
                "--dry-run and --status work on one episode: open it with --project and leave out --episode"
            ));
        }
        // a scene of a show export is a scene of each episode named
        let scenes = (!a.scenes.is_empty()).then(|| a.scenes.clone());
        let episodes: Vec<Value> = a
            .episodes
            .iter()
            .map(|e| json!({ "episode": e, "scenes": scenes }))
            .collect();
        return call(
            sess,
            "export_show",
            json!({ "episodes": episodes, "settings": settings, "preset": preset }),
        );
    }
    let scenes = (!a.scenes.is_empty()).then(|| a.scenes.clone());
    let method = if a.dry_run {
        "export_plan"
    } else {
        "export_scenes"
    };
    call(
        sess,
        method,
        json!({ "scenes": scenes, "status": status, "settings": settings, "preset": preset }),
    )
}

/// The service's word for what the command line spells with hyphens.
fn snake(word: &str) -> String {
    word.replace('-', "_")
}

#[derive(Subcommand, Debug)]
pub enum ShowCmd {
    /// Create a new show folder (`parent/name`) with Episodes/ and Sound/.
    New {
        name: String,
        #[arg(long, default_value = ".")]
        parent: PathBuf,
    },
    /// Make an existing production folder a show (writes <root>/<name>.cutawayshow only).
    Init {
        root: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// The show's episodes (number, title, whether they have a project).
    Episodes { root: PathBuf },
    /// Read the show's sound library again; new folders become buckets.
    Rescan { root: PathBuf },
    /// Set the show's backdrop image (its theme colours come from it).
    Backdrop {
        image: PathBuf,
        #[arg(long)]
        root: PathBuf,
    },
    /// Copy a standalone episode into an episode folder of the show (the source stays).
    Migrate {
        source: PathBuf,
        #[arg(long)]
        root: PathBuf,
        /// Episode folder name ("Episode 2").
        #[arg(long)]
        episode: String,
        /// Only report what would be copied.
        #[arg(long)]
        dry_run: bool,
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
    /// AI proposes a folder for each loose sound effect (one AI call). Nothing moves without --apply.
    CleanUp {
        /// Move the files in the shared pipeline as proposed.
        #[arg(long)]
        apply: bool,
    },
    /// The sounds placed in a scene, with their ids.
    List {
        #[arg(long)]
        scene: Option<String>,
    },
    /// Place a bucket or library sound in a scene.
    Add {
        asset: String,
        #[arg(long)]
        scene: String,
        /// The bucket it comes from.
        #[arg(long)]
        bucket: Option<String>,
        /// Starting depth, 0 close … 1 far.
        #[arg(long, default_value_t = 0.0)]
        depth: f64,
        /// Anchor to this line …
        #[arg(long)]
        line: Option<String>,
        /// … and word of it …
        #[arg(long)]
        word: Option<usize>,
        /// … at its end (the pause after a word) instead of its start.
        #[arg(long)]
        end: bool,
        /// Free scene time instead of an anchor.
        #[arg(long, allow_negative_numbers = true)]
        at: Option<f64>,
    },
    /// Move a sound onto an anchor, to free time, or by an offset.
    Move {
        id: String,
        #[arg(long)]
        line: Option<String>,
        #[arg(long)]
        word: Option<usize>,
        #[arg(long)]
        end: bool,
        #[arg(long, allow_negative_numbers = true)]
        at: Option<f64>,
        /// Seconds after the anchor or time.
        #[arg(long, allow_negative_numbers = true)]
        offset: Option<f64>,
        /// With --at: hang it on whatever is said at that moment.
        #[arg(long)]
        glue: bool,
    },
    /// Turn a sound's glue on where it stands, or with --off leave it at its time for good.
    Glue {
        id: String,
        #[arg(long)]
        off: bool,
    },
    /// Trim a sound's start or end by seconds (positive moves the edge later).
    Trim {
        id: String,
        /// start or end.
        edge: String,
        #[arg(allow_negative_numbers = true)]
        delta: f64,
    },
    /// Set a sound's fades. --out turns auto off, --auto hands the fade out back to the length.
    Fade {
        id: String,
        #[arg(long = "in")]
        fade_in: Option<f64>,
        #[arg(long = "out")]
        fade_out: Option<f64>,
        #[arg(long)]
        auto: bool,
    },
    /// Set a sound's loudness in dB (−24 … +6).
    Gain {
        id: String,
        #[arg(allow_negative_numbers = true)]
        db: f64,
    },
    /// Set a sound's pan (−1 left … +1 right).
    Pan {
        id: String,
        #[arg(allow_negative_numbers = true)]
        pan: f64,
    },
    /// Set how far back a sound sits (0 close … 1 far).
    Depth { id: String, depth: f64 },
    /// Slip the audio inside a sound's window: a new source in point.
    Slip { id: String, in_s: f64 },
    /// Take a sound off the timeline.
    Remove { id: String },
    /// Several edits as one undo step: JSON like
    /// '[{"op":"gain","placement_id":"pl_1","gain_db":-3},{"op":"depth", ...}]'.
    Batch { edits: String },
    /// Each scene's room.
    Rooms,
    /// How much of the scene's room a character's voice takes (0 dry … 1).
    VoiceWet { character: String, wet: f64 },
    /// Where a character stands in a scene (−1 left … +1 right), or with --room in its whole
    /// room. Without a value, it goes.
    VoicePan {
        scene: String,
        character: String,
        #[arg(allow_negative_numbers = true)]
        pan: Option<f64>,
        #[arg(long)]
        room: bool,
    },
    /// Loop a scene's room tone from a library sound. Without --sound it plays its location's.
    RoomTone {
        scene: String,
        #[arg(long)]
        sound: Option<String>,
        #[arg(long, allow_negative_numbers = true)]
        gain: Option<f64>,
    },
    /// Set Loop on a bucket sound: dropped, one over 6 s repeats on the ambience lane.
    Loop {
        bucket: String,
        sound: String,
        #[arg(long)]
        off: bool,
    },
    /// Set a scene's room (each 0 … 1; left out stays).
    Room {
        scene: String,
        #[arg(long)]
        size: Option<f64>,
        #[arg(long)]
        damping: Option<f64>,
        #[arg(long)]
        wet: Option<f64>,
    },
}

/// The edge of an anchor from the CLI's `--end`.
fn anchor_edge(end: bool) -> &'static str {
    if end {
        "end"
    } else {
        "start"
    }
}

/// The sound subcommands that are one service call.
fn sound_call(sess: &mut Session, cmd: &SfxCmd) -> Option<anyhow::Result<Value>> {
    Some(match cmd {
        SfxCmd::List { scene } => call(sess, "list_sounds", json!({ "scene": scene })),
        SfxCmd::Add {
            asset,
            scene,
            bucket,
            depth,
            line,
            word,
            end,
            at,
        } => call(
            sess,
            "place_from_bucket",
            json!({
                "scene": scene, "asset_id": asset, "bucket_id": bucket, "depth": depth,
                "anchor_line_id": line, "anchor_word": word, "anchor_edge": anchor_edge(*end),
                "at_s": at,
            }),
        ),
        SfxCmd::Move {
            id,
            line,
            word,
            end,
            at,
            offset,
            glue,
        } => call(
            sess,
            "move_sound",
            json!({
                "placement_id": id, "anchor_line_id": line, "anchor_word": word,
                "anchor_edge": anchor_edge(*end), "at_s": at, "offset_s": offset, "glue": glue,
            }),
        ),
        SfxCmd::Glue { id, off } => call(
            sess,
            "set_sound_glue",
            json!({ "placement_id": id, "glued": !off }),
        ),
        SfxCmd::Trim { id, edge, delta } => call(
            sess,
            "trim_sound",
            json!({ "placement_id": id, "edge": edge, "delta_s": delta }),
        ),
        SfxCmd::Fade {
            id,
            fade_in,
            fade_out,
            auto,
        } => call(
            sess,
            "set_sound_fades",
            json!({
                "placement_id": id, "fade_in_s": fade_in, "fade_out_s": fade_out,
                "auto": auto.then_some(true),
            }),
        ),
        SfxCmd::Gain { id, db } => call(
            sess,
            "set_sound_gain",
            json!({ "placement_id": id, "gain_db": db }),
        ),
        SfxCmd::Pan { id, pan } => call(
            sess,
            "set_sound_pan",
            json!({ "placement_id": id, "pan": pan }),
        ),
        SfxCmd::Depth { id, depth } => call(
            sess,
            "set_sound_depth",
            json!({ "placement_id": id, "depth": depth }),
        ),
        SfxCmd::Slip { id, in_s } => call(
            sess,
            "slip_sound",
            json!({ "placement_id": id, "in_s": in_s }),
        ),
        SfxCmd::Remove { id } => call(sess, "remove_sound", json!({ "placement_id": id })),
        SfxCmd::Batch { edits } => serde_json::from_str::<Value>(edits)
            .context("the edits are not valid JSON")
            .and_then(|edits| call(sess, "batch_sounds", json!({ "edits": edits }))),
        SfxCmd::Rooms => call(sess, "scene_rooms", json!({})),
        SfxCmd::VoiceWet { character, wet } => call(
            sess,
            "set_character_wet",
            json!({ "character": character, "wet": wet }),
        ),
        SfxCmd::VoicePan {
            scene,
            character,
            pan,
            room,
        } => call(
            sess,
            "set_character_pan",
            json!({ "scene": scene, "character": character, "pan": pan, "room": room }),
        ),
        SfxCmd::RoomTone { scene, sound, gain } => call(
            sess,
            "set_scene_tone",
            json!({ "scene": scene, "asset_id": sound, "gain_db": gain }),
        ),
        SfxCmd::Loop { bucket, sound, off } => call(
            sess,
            "bucket_set_loop",
            json!({ "bucket_id": bucket, "asset_id": sound, "on": !off }),
        ),
        SfxCmd::Room {
            scene,
            size,
            damping,
            wet,
        } => call(
            sess,
            "set_scene_room",
            json!({ "scene": scene, "size": size, "damping": damping, "wet": wet }),
        ),
        _ => return None,
    })
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
        Cmd::Show { cmd } => {
            let mut sess = Session::new("cli");
            let show = |sess: &mut Session, root: &Path| {
                call(sess, "open_show", json!({ "path": s(root) }))
            };
            match cmd {
                ShowCmd::New { .. } => unreachable!("handled above"),
                ShowCmd::Init { root, name } => call(
                    &mut sess,
                    "create_show",
                    json!({ "root": s(root), "name": name }),
                ),
                ShowCmd::Episodes { root } => show(&mut sess, root).map(|v| v["episodes"].clone()),
                ShowCmd::Rescan { root } => {
                    show(&mut sess, root)?;
                    call(&mut sess, "rescan_library", json!({}))
                }
                ShowCmd::Backdrop { image, root } => {
                    show(&mut sess, root)?;
                    call(&mut sess, "set_show_backdrop", json!({ "path": s(image) }))
                }
                ShowCmd::Migrate {
                    source,
                    root,
                    episode,
                    dry_run,
                } => {
                    show(&mut sess, root)?;
                    call(
                        &mut sess,
                        "migrate_episode",
                        json!({ "source": s(source), "episode": episode, "dry_run": dry_run }),
                    )
                }
            }
        }
        Cmd::Shelf { paths } => {
            let recents: Vec<Value> = paths
                .iter()
                .map(|p| json!({ "path": s(p), "show": apc_app::welcome::is_show_path(p) }))
                .collect();
            let mut sess = Session::new("cli");
            call(&mut sess, "welcome_view", json!({ "recents": recents }))
        }
        Cmd::Cover { target, image } => {
            let mut sess = Session::new("cli");
            call(
                &mut sess,
                "set_cover",
                json!({ "target": s(target), "image": s(image) }),
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
                    cmd: SfxCmd::CleanUp { apply: false },
                } => call(&mut sess, "plan_cleanup", json!({})),
                Cmd::Sfx {
                    cmd: SfxCmd::CleanUp { apply: true },
                } => {
                    let plan = call(&mut sess, "plan_cleanup", json!({}))?;
                    call(
                        &mut sess,
                        "apply_cleanup",
                        json!({ "moves": plan["moves"] }),
                    )
                }
                Cmd::Sfx {
                    cmd: SfxCmd::Import { paths },
                } => call(
                    &mut sess,
                    "import_sounds",
                    json!({ "paths": paths.iter().map(|p| s(p)).collect::<Vec<_>>() }),
                ),
                Cmd::Sfx { cmd } => match sound_call(&mut sess, cmd) {
                    Some(r) => r,
                    None => Err(anyhow!("unknown sound command")),
                },
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
                Cmd::Deliver(a) => deliver(&mut sess, a),
                Cmd::Lock { scene } => call(&mut sess, "lock_scene", json!({ "scene": scene })),
                Cmd::Unlock { scene } => call(
                    &mut sess,
                    "unlock_scene",
                    json!({ "scene": scene, "confirm": confirm }),
                ),
                Cmd::Beside { line, with, offset } => call(
                    &mut sess,
                    "set_beside",
                    json!({ "line_id": line, "beside": with, "offset_s": offset }),
                ),
                Cmd::Depth { depth, lines } => call(
                    &mut sess,
                    "set_line_depth",
                    json!({ "line_ids": lines, "depth": depth }),
                ),
                Cmd::Mark { scene, status } => call(
                    &mut sess,
                    "set_scene_status",
                    json!({ "scene": scene, "status": status }),
                ),
                Cmd::Note { cmd } => match cmd {
                    NoteCmd::Add {
                        text,
                        scene,
                        at,
                        to,
                        line,
                        word,
                        to_word,
                        sound,
                        tag,
                        author,
                    } => call(
                        &mut sess,
                        "add_note",
                        json!({
                            "text": text, "scene": scene, "at_s": at, "to_s": to, "line_id": line,
                            "word": word, "to_word": to_word, "placement_id": sound, "tag": tag,
                            "author": author.json(),
                        }),
                    ),
                    NoteCmd::List {
                        scene,
                        status,
                        tag,
                        author,
                        round,
                    } => call(
                        &mut sess,
                        "list_notes",
                        json!({ "scene": scene, "status": status, "tag": tag, "author": author, "round": round }),
                    ),
                    NoteCmd::Reply { note, text, author } => call(
                        &mut sess,
                        "reply_note",
                        json!({ "note": note, "text": text, "author": author.json() }),
                    ),
                    NoteCmd::Edit { note, text, author } => call(
                        &mut sess,
                        "edit_note",
                        json!({ "note": note, "text": text, "author": author.json() }),
                    ),
                    NoteCmd::Tag { note, tag, author } => call(
                        &mut sess,
                        "tag_note",
                        json!({ "note": note, "tag": tag, "author": author.json() }),
                    ),
                    NoteCmd::Move {
                        note,
                        scene,
                        at,
                        to,
                        line,
                        word,
                        to_word,
                        sound,
                        author,
                    } => call(
                        &mut sess,
                        "move_note",
                        json!({
                            "note": note, "scene": scene, "at_s": at, "to_s": to, "line_id": line,
                            "word": word, "to_word": to_word, "placement_id": sound,
                            "author": author.json(),
                        }),
                    ),
                    NoteCmd::Remove { note, author } => call(
                        &mut sess,
                        "remove_note",
                        json!({ "note": note, "author": author.json(), "confirm": confirm }),
                    ),
                    NoteCmd::Resolve { note, author } => call(
                        &mut sess,
                        "resolve_note",
                        json!({ "note": note, "author": author.json() }),
                    ),
                    NoteCmd::Reopen { note, author } => call(
                        &mut sess,
                        "reopen_note",
                        json!({ "note": note, "author": author.json() }),
                    ),
                },
                Cmd::Review { cmd } => match cmd {
                    ReviewCmd::Start { scenes } => {
                        call(&mut sess, "start_review_round", json!({ "scenes": scenes }))
                    }
                    ReviewCmd::Close { round } => {
                        call(&mut sess, "close_review_round", json!({ "round": round }))
                    }
                    ReviewCmd::Who {
                        name: Some(name),
                        role,
                        remember,
                    } => call(
                        &mut sess,
                        "set_reviewer",
                        json!({ "name": name, "role": role, "remember": remember }),
                    ),
                    ReviewCmd::Who { name: None, .. } => call(&mut sess, "reviewer", json!({})),
                    ReviewCmd::History => call(&mut sess, "review_history", json!({})),
                    ReviewCmd::Search { query } => {
                        call(&mut sess, "review_search", json!({ "q": query }))
                    }
                    ReviewCmd::Sync => call(&mut sess, "review_sync", json!({})),
                    ReviewCmd::Add { file, anyway } => call(
                        &mut sess,
                        "add_review",
                        json!({ "path": s(file), "anyway": anyway }),
                    ),
                    ReviewCmd::Send { path } => {
                        call(&mut sess, "send_review", json!({ "path": s(path) }))
                    }
                },
                Cmd::Exports => call(&mut sess, "export_status", json!({})),
                Cmd::Pipeline { set } => match set {
                    Some(p) => call(
                        &mut sess,
                        "set_pipeline_folder",
                        json!({ "path": s(p), "confirm": confirm }),
                    ),
                    None => call(&mut sess, "pipeline_folder", json!({})),
                },
                Cmd::Preset {
                    name,
                    delete,
                    settings,
                } => {
                    if *delete {
                        call(&mut sess, "delete_export_preset", json!({ "name": name }))
                    } else {
                        let settings = settings.to_json()?;
                        call(
                            &mut sess,
                            "save_export_preset",
                            json!({ "name": name, "settings": settings }),
                        )
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(args: &[&str]) -> anyhow::Result<Value> {
        let line = [&["cutaway", "deliver"], args].concat();
        match Cli::try_parse_from(line)?.cmd {
            Cmd::Deliver(a) => a.settings.to_json(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_defaults_are_the_pages() {
        let s: apc_core::config::ExportSettings =
            serde_json::from_value(settings(&[]).unwrap()).unwrap();
        assert_eq!(s, Default::default());
    }

    #[test]
    fn loudness_takes_a_negative_number_or_off() {
        assert_eq!(settings(&["--loudness", "-23"]).unwrap()["loudness"], -23.0);
        assert_eq!(
            settings(&["--loudness", "off"]).unwrap()["loudness"],
            Value::Null
        );
        assert!(settings(&["--loudness", "loud"]).is_err());
    }

    #[test]
    fn one_file_is_a_mode_of_its_own() {
        let s: apc_core::config::ExportSettings =
            serde_json::from_value(settings(&["--as", "one-file", "--format", "mp3"]).unwrap())
                .unwrap();
        assert_eq!(s.mode, apc_core::config::ExportMode::OneFile);
        assert!(s.check().is_ok(), "one file is a mix, so MP3 is fine");
    }

    #[test]
    fn tracks_narrow_the_stack() {
        let s = settings(&["--tracks", "dialogue,mix"]).unwrap();
        assert_eq!(s["tracks"]["dialogue"], true);
        assert_eq!(s["tracks"]["music"], false);
        assert!(settings(&["--tracks", "voices"]).is_err());
    }
}
