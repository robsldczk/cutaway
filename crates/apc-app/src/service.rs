//! The service: one open episode with its command bus, backups, providers and caches.
//! The UI (Tauri), the CLI, the MCP server and the HTTP dev bridge all call `Service::call`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};

use apc_ai::{AiProvider, AiRunner, ClaudeCodeProvider, MockProvider};
use apc_analysis::asr::{AsrProvider, TranscriptImportProvider};
use apc_analysis::roles::guess_by_name;
use apc_analysis::MatchReport;
use apc_core::backup::{BackupManager, RotationPolicy, SystemClock};
use apc_core::config::ProjectConfig;
use apc_core::ids::new_id;
use apc_core::project::{self, EpisodePaths};
use apc_core::store::EpisodeStore;
use apc_core::*;
use apc_media::AudioAnalysis;
use apc_sound::{Library, SharedShowLibrary, ShowLibrary};

use crate::error::{AppError, Result};

/// Where a client comes from (logged with every command).
pub const CLIENTS: &[&str] = &["ui", "cli", "mcp", "pipeline", "test"];

#[derive(Debug, Clone, Serialize)]
pub struct ImportedSource {
    pub source_id: String,
    pub file: String,
    pub role: Role,
    pub character: Option<String>,
    pub confidence: f64,
    pub reason: String,
}

pub struct Service {
    pub paths: EpisodePaths,
    pub bus: CommandBus,
    pub backups: BackupManager,
    pub client: String,
    media: HashMap<String, Arc<AudioAnalysis>>,
    pub last_report: Option<MatchReport>,
    pub jobs: crate::jobs::Jobs,
    pub player: crate::player::Player,
    /// API keys of sound services (OS credential store; in memory with `CUTAWAY_SECRETS=memory`).
    pub secrets: Arc<dyn apc_sound::secrets::SecretStore>,
    /// The show's sound library, when the episode lives in a show.
    pub show_lib: Option<SharedShowLibrary>,
    /// Things to tell the user once (the episode is open on another machine, sync conflicts).
    pub warnings: Vec<String>,
    /// This UI session holds the episode's lock file (studio mode).
    locked: bool,
    /// Who's reviewing, and how this session stands to the review files.
    pub review: crate::review::ReviewLocal,
}

fn default_secrets() -> Arc<dyn apc_sound::secrets::SecretStore> {
    if std::env::var("CUTAWAY_SECRETS").as_deref() == Ok("memory") {
        Arc::new(apc_sound::secrets::MemoryStore::default())
    } else {
        Arc::new(apc_sound::secrets::Keychain::new())
    }
}

impl Service {
    /// The API key of a sound service (credential store, then environment). Never leaves the core.
    pub fn api_key(&self, s: apc_sound::secrets::KeyedService) -> Option<String> {
        apc_sound::secrets::api_key(self.secrets.as_ref(), s)
    }

    /// Opens an episode file (or the folder containing it).
    pub fn open(path: &Path, client: &str) -> Result<Self> {
        let file = project::find_episode_file(path)?;
        let paths = EpisodePaths::for_file(&file)?;
        paths.ensure_dirs()?;
        let store = EpisodeStore::open(&file)?;
        Self::from_store(paths, store, client)
    }

    /// Creates an episode (inside a show when `show` is given).
    pub fn create(parent: &Path, name: &str, show: Option<&Path>, client: &str) -> Result<Self> {
        let (paths, store) = project::create_episode(parent, name, show)?;
        Self::from_store(paths, store, client)
    }

    /// A session on a private copy of someone's package (review mode): nothing in the package
    /// is written, so it takes no lock and makes no backups.
    pub(crate) fn from_private(
        paths: EpisodePaths,
        store: EpisodeStore,
        client: &str,
    ) -> Result<Self> {
        Self::start(paths, store, client, true)
    }

    fn from_store(paths: EpisodePaths, store: EpisodeStore, client: &str) -> Result<Self> {
        Self::start(paths, store, client, false)
    }

    fn start(
        paths: EpisodePaths,
        store: EpisodeStore,
        client: &str,
        private: bool,
    ) -> Result<Self> {
        let bus = CommandBus::new(store, client)?;
        let backups = BackupManager::new(&paths, RotationPolicy::default(), Box::new(SystemClock));
        let show_lib = match &paths.show {
            Some(show) => Some(Arc::new(std::sync::Mutex::new(ShowLibrary::open(show)?.0))),
            None => None,
        };
        let mut svc = Service {
            paths,
            bus,
            backups,
            client: client.to_string(),
            media: HashMap::new(),
            last_report: None,
            jobs: crate::jobs::Jobs::default(),
            player: crate::player::Player::default(),
            secrets: default_secrets(),
            show_lib,
            warnings: Vec::new(),
            locked: false,
            review: crate::review::ReviewLocal::new(private),
        };
        if svc.paths.show.is_some() {
            svc.warnings = crate::studio::sync_warnings(&svc.paths);
            // the UI holds the lock; the CLI, MCP and background jobs share the episode
            if client == "ui" && !private {
                if let Some(w) = crate::studio::take_lock(&svc.paths)? {
                    svc.warnings.insert(0, w);
                }
                svc.locked = true;
            }
            crate::studio::link_folders(&mut svc)?;
            crate::studio::relink_sounds(&mut svc)?;
        }
        Ok(svc)
    }

    pub fn doc(&self) -> &Document {
        self.bus.doc()
    }

    pub fn name(&self) -> String {
        self.bus
            .store()
            .meta("name")
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                self.paths
                    .root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
    }

    pub fn config(&self) -> ProjectConfig {
        ProjectConfig::from_doc(self.doc())
    }

    /// The bus takes notes and nothing else.
    pub fn in_review(&self) -> bool {
        self.bus.mode() == apc_core::bus::BusMode::Review
    }

    /// Executes a command and runs the autosave check.
    pub fn exec(&mut self, cmd: &Command) -> Result<Executed> {
        let r = self.bus.execute(cmd)?;
        if r.seq.is_some() {
            self.after_write()?;
        }
        Ok(r)
    }

    /// Before the episode is closed: stop playback and snapshot unsaved changes into
    /// `Backups/` (every command is already on disk; this keeps the last minutes restorable).
    pub fn close(&mut self) -> Result<Value> {
        crate::player::stop(self)?;
        if self.review.private {
            // the private copy stays for next time; the package was never touched
            return Ok(json!({ "file": self.paths.file.to_string_lossy(), "snapshot": null }));
        }
        let snap = self.backups.flush(self.bus.store())?;
        // a sync tool copies the episode file alone: fold the log into it
        self.bus.store().checkpoint()?;
        if self.locked {
            crate::studio::release_lock(&self.paths);
            self.locked = false;
        }
        Ok(json!({ "file": self.paths.file.to_string_lossy(), "snapshot": snap.map(|b| b.file) }))
    }

    pub fn after_write(&mut self) -> Result<()> {
        if self.review.private {
            return Ok(());
        }
        self.backups.note_change();
        if self.backups.maybe_autosave(self.bus.store())?.is_some() && self.paths.show.is_some() {
            self.bus.store().checkpoint()?;
        }
        Ok(())
    }

    pub fn library(&self) -> Library {
        Library::new(self.paths.library.clone(), self.show_lib.clone())
    }

    pub fn aliases(&self) -> BTreeMap<String, String> {
        let mut m: BTreeMap<String, String> = BTreeMap::new();
        for c in self.doc().characters.values() {
            for a in &c.aliases {
                m.insert(a.clone(), c.id.clone());
            }
        }
        if let Some(show) = &self.paths.show {
            m.extend(show.data.aliases.clone());
        }
        m
    }

    /// The AI provider from the config (`CUTAWAY_AI_PROVIDER` overrides). Falls back to the
    /// mock rules when the Claude Code CLI is not installed.
    pub fn ai_provider(&self) -> (Arc<dyn AiProvider>, Option<String>) {
        let cfg = self.config();
        let wanted = std::env::var("CUTAWAY_AI_PROVIDER").unwrap_or(cfg.ai.provider.clone());
        match wanted.as_str() {
            "mock" => (Arc::new(MockProvider::new()), None),
            _ => {
                let p = ClaudeCodeProvider::new(&cfg.ai.model, &cfg.ai.effort, cfg.ai.timeout_s);
                if p.available() {
                    (Arc::new(p), None)
                } else {
                    (
                        Arc::new(MockProvider::new()),
                        Some("Claude Code CLI not found; using the built-in rules for notes and picks".into()),
                    )
                }
            }
        }
    }

    pub fn ai_runner(&self) -> (AiRunner, Option<String>) {
        let (p, warn) = self.ai_provider();
        (
            AiRunner::new(
                p,
                Some(self.paths.cache.join("ai")),
                Some(self.paths.logs.join("ai-calls.jsonl")),
            ),
            warn,
        )
    }

    /// Transcript import when every source has a sidecar, otherwise Parakeet.
    pub fn asr_provider(&self) -> Box<dyn AsrProvider> {
        let all_sidecars = self
            .doc()
            .sources
            .values()
            .filter(|s| s.role != Role::Unknown)
            .all(|s| s.transcript.is_some());
        if all_sidecars {
            return Box::new(TranscriptImportProvider);
        }
        #[cfg(feature = "parakeet")]
        {
            Box::new(apc_analysis::parakeet::ParakeetAsr::new(
                apc_analysis::asr::ModelManager::new(
                    apc_analysis::asr::ModelManager::default_root(),
                ),
            ))
        }
        #[cfg(not(feature = "parakeet"))]
        {
            Box::new(apc_analysis::asr::ParakeetProvider {
                models: apc_analysis::asr::ModelManager::new(
                    apc_analysis::asr::ModelManager::default_root(),
                ),
            })
        }
    }

    /// Downloads the speech model once, with progress, before an analysis needs it (instead
    /// of a silent ~490 MB download behind "Transcribing …").
    pub fn prepare_asr(&self, progress: &dyn Fn(apc_analysis::Progress)) -> Result<()> {
        let all_sidecars = self
            .doc()
            .sources
            .values()
            .filter(|s| s.role != Role::Unknown)
            .all(|s| s.transcript.is_some());
        let models =
            apc_analysis::asr::ModelManager::new(apc_analysis::asr::ModelManager::default_root());
        let spec = &apc_analysis::asr::PARAKEET_TDT_V3;
        if all_sidecars || !cfg!(feature = "parakeet") || models.is_installed(spec) {
            return Ok(());
        }
        models
            .ensure(spec, &|_, got, total| {
                let mb = got as f64 / 1e6;
                let (fraction, message) = match total {
                    Some(t) if t > 0 => (
                        got as f64 / t as f64,
                        format!(
                            "Downloading the speech model (once): {mb:.0} of {:.0} MB",
                            t as f64 / 1e6
                        ),
                    ),
                    _ => (
                        0.0,
                        format!("Downloading the speech model (once): {mb:.0} MB"),
                    ),
                };
                progress(apc_analysis::Progress {
                    stage: "model".into(),
                    fraction,
                    message,
                });
            })
            .map_err(|e| {
                AppError::Other(format!("the speech model could not be downloaded: {e}"))
            })?;
        Ok(())
    }

    /// Cached audio analysis (envelope, peaks) of a source.
    pub fn analysis_of(&mut self, source_id: &str) -> Option<Arc<AudioAnalysis>> {
        if let Some(a) = self.media.get(source_id) {
            return Some(a.clone());
        }
        let (path, hash) = if let Some(aid) = source_id.strip_prefix("asset:") {
            let a = self.doc().assets.get(aid)?;
            (self.library().path_of(a), a.hash.clone())
        } else {
            let s = self.doc().sources.get(source_id)?;
            (self.paths.resolve(&s.path), s.hash.clone())
        };
        let a =
            Arc::new(AudioAnalysis::cached(&path, &hash, &self.paths.cache.join("media")).ok()?);
        self.media.insert(source_id.to_string(), a.clone());
        Some(a)
    }

    /// Imports a script (Fountain or Throughline `.thrl`) as the next version.
    pub fn import_script(&mut self, path: &Path, name: Option<&str>) -> Result<Value> {
        let parsed = apc_core::script::read_script_file(path)?;
        let number = self
            .doc()
            .script_versions
            .values()
            .map(|v| v.number)
            .max()
            .unwrap_or(0)
            + 1;
        let name = name
            .map(String::from)
            .or_else(|| parsed.title.get("Title").cloned())
            .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "Script".into());
        let (file, hash) = apc_core::script::store_version_file(&self.paths, path, &name, number)?;
        let version = ScriptVersion {
            id: new_id("sv"),
            number,
            file,
            name,
            hash,
            imported_at: chrono::Utc::now().to_rfc3339(),
            summary: Value::Null,
            title: parsed.title.clone(),
        };
        let r = self.exec(&Command::ImportScriptVersion { version, parsed })?;
        Ok(r.result)
    }

    /// Takes only the story context (character bios, settings) from a script file.
    pub fn import_story(&mut self, path: &Path) -> Result<Value> {
        let story = apc_core::script::read_script_file(path)?.story;
        if story.is_empty() {
            return Err(AppError::Params(format!(
                "{} holds no story context (characters or settings)",
                path.display()
            )));
        }
        let r = self.exec(&Command::ImportStoryContext { story })?;
        Ok(r.result)
    }

    /// Imports audio files or folders and assigns roles (name, aliases, room keywords,
    /// then AI name matching for files the rules can't place).
    pub fn import_audio(&mut self, inputs: &[PathBuf], copy: bool) -> Result<Vec<ImportedSource>> {
        // in a show the episode travels through the pipeline: no paths outside it
        let copy = copy || self.paths.show.is_some();
        let mut files = Vec::new();
        for p in inputs {
            files.extend(apc_core::media::audio_files_in(p)?);
        }
        if files.is_empty() {
            return Err(AppError::Params("no audio files found".into()));
        }
        let folder = format!("Session {}", chrono::Local::now().format("%Y-%m-%d"));
        let characters: Vec<String> = self.doc().characters.keys().cloned().collect();
        let aliases = self.aliases();
        let mut sources = Vec::new();
        let mut report = Vec::new();
        for f in &files {
            let imp = apc_core::media::import_file(&self.paths, f, &folder, copy)?;
            let info = apc_media::decode::probe(f).ok();
            let mut g = guess_by_name(&imp.original_name, &characters, &aliases);
            if let Some(existing) = self.doc().sources.values().find(|s| s.hash == imp.hash) {
                report.push(ImportedSource {
                    source_id: existing.id.clone(),
                    file: imp.original_name.clone(),
                    role: existing.role,
                    character: existing.character.clone(),
                    confidence: existing.role_confidence,
                    reason: "already imported".into(),
                });
                continue;
            }
            if g.role == Role::Unknown {
                g = self
                    .ai_name_guess(&imp.original_name, &characters, &aliases)
                    .unwrap_or(g);
            }
            let id = new_id("src");
            report.push(ImportedSource {
                source_id: id.clone(),
                file: imp.original_name.clone(),
                role: g.role,
                character: g.character.clone(),
                confidence: g.confidence,
                reason: g.reason.clone(),
            });
            sources.push(SourceFile {
                id,
                path: imp.stored_path,
                original_name: imp.original_name,
                stored: imp.stored,
                hash: imp.hash,
                size: imp.size,
                role: g.role,
                character: g.character,
                role_confidence: g.confidence,
                session_id: None,
                sample_rate: info.map(|i| i.sample_rate).unwrap_or(0),
                channels: info.map(|i| i.channels).unwrap_or(0),
                duration_s: info.map(|i| i.duration_s).unwrap_or(0.0),
                tc_start_s: apc_media::bwf::read_bwf(f)
                    .ok()
                    .flatten()
                    .and_then(|b| b.tc_start_s()),
                offset_s: 0.0,
                offset_confidence: 0.0,
                transcript: imp.sidecar,
                missing: false,
                link_locked: false,
            });
        }
        if !sources.is_empty() {
            self.exec(&Command::AddSources { sources })?;
        }
        Ok(report)
    }

    pub(crate) fn ai_name_guess(
        &self,
        file: &str,
        characters: &[String],
        aliases: &BTreeMap<String, String>,
    ) -> Option<apc_analysis::roles::RoleGuess> {
        let (runner, _) = self.ai_runner();
        let character_notes = if self.config().ai.story_context {
            self.doc()
                .characters
                .values()
                .map(|c| (c.id.clone(), c.profile.summary.clone()))
                .filter(|(_, s)| !s.trim().is_empty())
                .collect()
        } else {
            Default::default()
        };
        let out = runner
            .run(&apc_ai::NameMatchInput {
                files: vec![file.into()],
                characters: characters.to_vec(),
                aliases: aliases.clone(),
                character_notes,
            })
            .ok()?
            .output?;
        let a = out.assignments.into_iter().next()?;
        let role = match a.role {
            apc_ai::NameRole::Character => Role::Character,
            apc_ai::NameRole::Room => Role::Room,
            apc_ai::NameRole::Unknown => return None,
        };
        Some(apc_analysis::roles::RoleGuess {
            file: file.into(),
            role,
            character: a.character,
            confidence: a.confidence.min(0.6),
            reason: "AI name match".into(),
        })
    }

    /// Applies role corrections and remembers aliases at show level.
    pub fn confirm_roles(
        &mut self,
        items: &[(String, Role, Option<String>)],
        remember: bool,
    ) -> Result<Value> {
        for (sid, role, ch) in items {
            let before = self
                .doc()
                .sources
                .get(sid)
                .cloned()
                .ok_or_else(|| AppError::Params(format!("no source {sid}")))?;
            self.exec(&Command::SetRole {
                source_id: sid.clone(),
                role: *role,
                character: ch.clone(),
            })?;
            if let (Role::Character, Some(ch), true) = (role, ch, remember) {
                if before.character.as_deref() != Some(ch.as_str()) {
                    if let Some(tok) = before
                        .original_name
                        .split(|c: char| !c.is_alphanumeric())
                        .find(|t| t.len() >= 2)
                    {
                        if !ch.to_lowercase().contains(&tok.to_lowercase()) {
                            if let Some(show) = self.paths.show.as_mut() {
                                show.update(|d| {
                                    d.aliases.insert(tok.to_string(), ch.clone());
                                })?;
                            }
                        }
                    }
                }
            }
        }
        Ok(json!({ "updated": items.len() }))
    }

    /// Runs the whole analysis and applies it as one undoable command.
    pub fn analyze(&mut self, progress: &dyn Fn(apc_analysis::Progress)) -> Result<MatchReport> {
        let scope = self.configured(apc_analysis::AnalysisScope::everything());
        self.analyze_with(scope, progress)
    }

    /// A scope with the project's matching settings, glossary and rescue examples.
    pub fn configured(
        &self,
        mut scope: apc_analysis::AnalysisScope,
    ) -> apc_analysis::AnalysisScope {
        let cfg = self.config();
        scope.rescue = scope.rescue && cfg.ai.rescue;
        scope.tuning = apc_analysis::pipeline::Tuning {
            merge_gap_s: cfg.analysis.merge_gap_s,
            bleed_margin_db: cfg.analysis.bleed_margin_db,
            match_offset: cfg.analysis.match_offset,
            glossary: cfg
                .analysis
                .glossary
                .iter()
                .map(|g| (g.heard.clone(), g.script.clone()))
                .collect(),
        };
        scope.examples = cfg.ai.rescue_examples.clone();
        for g in &cfg.analysis.glossary {
            scope.examples.push(format!(
                "The speech recogniser often hears “{}” where the script says “{}”.",
                g.heard, g.script
            ));
        }
        scope
    }

    /// Runs the analysis for a scope (some characters, some lines, or dev-panel marks).
    /// Marks get their proposals stored; everything else is applied as one undo step.
    pub fn analyze_with(
        &mut self,
        scope: apc_analysis::AnalysisScope,
        progress: &dyn Fn(apc_analysis::Progress),
    ) -> Result<MatchReport> {
        self.bus.refresh()?;
        // it would only be refused at the end, after the long part
        if scope.characters.is_empty() && scope.line_ids.is_empty() && scope.marks.is_empty() {
            if let Some(refusal) = self
                .doc()
                .scenes_in_order()
                .iter()
                .find_map(|s| s.lock_refusal())
            {
                return Err(apc_core::CoreError::SceneLocked(refusal).into());
            }
        }
        self.prepare_asr(progress)?;
        let (ai, warn) = self.ai_runner();
        let asr = self.asr_provider();
        let out = apc_analysis::analyze_scoped(
            self.doc(),
            &self.paths,
            &apc_analysis::Providers {
                asr: asr.as_ref(),
                ai: &ai,
            },
            &scope,
            progress,
        )?;
        let mut report = out.report;
        if let Some(w) = warn {
            report.flagged.insert(0, w);
        }
        if !scope.marks.is_empty() {
            for (id, proposal) in out.proposals {
                if let Some(mut m) = self.doc().marks.get(&id).cloned() {
                    m.proposal = Some(proposal);
                    m.status = MarkStatus::Proposed;
                    self.exec(&Command::PutMark { mark: m })?;
                }
            }
            self.last_report = Some(report.clone());
            return Ok(report);
        }
        self.exec(&Command::ApplyAnalysis {
            payload: out.payload,
        })?;
        std::fs::write(self.paths.logs.join("match-report.md"), &report.markdown)?;
        self.media.clear();
        self.last_report = Some(report.clone());
        Ok(report)
    }
}
