use anyhow::Result;
use clap::{Parser, Subcommand};

use colored::Colorize;

use crate::config::GritConfig;
use crate::db::azure_store::AzureConfig;
use crate::db::lock_store::{LockEntry, LockResult, LockStore};
use crate::db::s3_store::S3Config;
use crate::db::sqlite_store::SqliteLockStore;
use crate::db::Database;
use crate::git::GitRepo;
use crate::parser::SymbolIndex;
use crate::room::{EventType, NotificationServer, Room, RoomEvent};

#[derive(Parser)]
#[command(
    name = "grit",
    version,
    about = "Coordination layer for parallel AI agents on top of git"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to the git repository (default: current directory)
    #[arg(short, long, default_value = ".")]
    pub repo: String,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize grit in the current repo
    Init,

    /// Claim symbols before working on them
    Claim {
        /// Agent identifier
        #[arg(short, long)]
        agent: String,

        /// Intent description (what you plan to do)
        #[arg(short, long)]
        intent: String,

        /// TTL in seconds (default: 600)
        #[arg(long, default_value = "600")]
        ttl: u64,

        /// Wait timeout in seconds if blocked (retries with backoff until granted or timeout)
        #[arg(short, long, default_value = "0")]
        wait: u64,

        /// Lock mode: "read" for shared access, "write" for exclusive (default: write)
        #[arg(long, default_value = "write")]
        mode: String,

        /// If blocked, queue up instead of failing (FIFO, auto-granted when released)
        #[arg(long)]
        queue: bool,

        /// Also lock all callees as read (dependency-aware locking)
        #[arg(long)]
        with_deps: bool,

        /// Symbols to claim (e.g. "auth.ts::login" "utils.ts::hash")
        symbols: Vec<String>,
    },

    /// Release symbols after finishing work
    Release {
        /// Agent identifier
        #[arg(short, long)]
        agent: String,

        /// Specific symbols to release (default: all held by agent)
        symbols: Vec<String>,
    },

    /// Show current lock status
    Status,

    /// List all symbols in the codebase
    Symbols {
        /// Filter by file path pattern
        #[arg(short, long)]
        file: Option<String>,
    },

    /// Agent declares intent and gets smart suggestions
    Plan {
        /// Agent identifier
        #[arg(short, long)]
        agent: String,

        /// What the agent wants to do
        #[arg(short, long)]
        intent: String,
    },

    /// Mark agent as done, merge worktree, release all locks
    Done {
        /// Agent identifier
        #[arg(short, long)]
        agent: String,
    },

    /// Watch real-time events from the room socket (or poll S3)
    Watch {
        /// Poll interval in seconds for S3 backend (default: uses Unix socket for local)
        #[arg(long)]
        poll: Option<u64>,
    },

    /// Manage git worktrees
    Worktree {
        #[command(subcommand)]
        action: WorktreeAction,
    },

    /// Manage the lock queue
    Queue {
        #[command(subcommand)]
        action: QueueAction,
    },

    /// Garbage-collect expired locks
    Gc,

    /// Manage grit sessions (feature branches for multi-agent work)
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },

    /// Configure grit backend (local, s3, r2, gcs, azure)
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Auto-pick and claim a free symbol from a file
    Assign {
        /// Agent identifier
        #[arg(short, long)]
        agent: String,

        /// Intent description (what you plan to do)
        #[arg(short, long)]
        intent: String,

        /// File pattern to search for symbols
        #[arg(short, long)]
        file: String,

        /// TTL in seconds (default: 600)
        #[arg(long, default_value = "600")]
        ttl: u64,

        /// Lock mode: "read" for shared access, "write" for exclusive (default: write)
        #[arg(long, default_value = "write")]
        mode: String,
    },

    /// Refresh an agent's lock TTL
    Heartbeat {
        /// Agent identifier
        #[arg(short, long)]
        agent: String,

        /// New TTL in seconds (default: 600)
        #[arg(long, default_value = "600")]
        ttl: u64,
    },
}

#[derive(Subcommand)]
pub enum WorktreeAction {
    /// List active worktrees
    List,
}

#[derive(Subcommand)]
pub enum SessionAction {
    /// Start a new session (creates a feature branch)
    Start {
        /// Session name (becomes branch grit/<name>)
        name: String,
    },
    /// Show current session info
    Status,
    /// Create a PR for the current session
    Pr {
        /// PR title (default: session name)
        #[arg(short, long)]
        title: Option<String>,
    },
    /// End session (close locks, switch back to base branch)
    End {
        /// Session name
        name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum QueueAction {
    /// List all queued agents
    List,
    /// Cancel a queue entry for an agent
    Cancel {
        /// Agent identifier
        #[arg(short, long)]
        agent: String,
        /// Symbol to dequeue from (optional: if omitted, dequeues all)
        symbol: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set backend to S3-compatible storage
    SetS3 {
        /// S3 bucket name
        #[arg(long)]
        bucket: String,
        /// Custom endpoint (for R2, GCS, Azure, MinIO)
        #[arg(long)]
        endpoint: Option<String>,
        /// Region
        #[arg(long, default_value = "auto")]
        region: String,
    },
    /// Set backend to Azure Blob Storage (native API with Event Grid)
    SetAzure {
        /// Storage account name
        #[arg(long)]
        account: String,
        /// Access key
        #[arg(long)]
        access_key: String,
        /// Container name
        #[arg(long, default_value = "grit-locks")]
        container: String,
    },
    /// Set backend to local SQLite (default)
    SetLocal,
    /// Show current config
    Show,
}

/// Compute lock expiry locally from the LockEntry data, avoiding extra round-trips
fn is_entry_expired_local(entry: &LockEntry) -> bool {
    if let Ok(locked_at) = chrono::DateTime::parse_from_rfc3339(&entry.locked_at) {
        let elapsed = chrono::Utc::now().signed_duration_since(locked_at);
        elapsed.num_seconds() as u64 > entry.ttl_seconds
    } else {
        true
    }
}

/// Validate agent/session identifiers to prevent path traversal and argument injection
fn validate_identifier(id: &str, label: &str) -> Result<()> {
    if id.is_empty() {
        anyhow::bail!("Invalid {}: must not be empty", label);
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") || id.starts_with('-') {
        anyhow::bail!(
            "Invalid {}: '{}' contains forbidden characters (/, \\, ..) or starts with -",
            label,
            id
        );
    }
    if id == "." || id.starts_with('.') || id.ends_with('.') {
        anyhow::bail!(
            "Invalid {}: '{}' must not be '.', start with '.', or end with '.'",
            label,
            id
        );
    }
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        anyhow::bail!(
            "Invalid {}: '{}' must contain only alphanumeric, hyphens, underscores, dots",
            label,
            id
        );
    }
    Ok(())
}

pub fn run(cli: Cli) -> Result<()> {
    // Validate identifiers early to prevent path traversal / argument injection
    match &cli.command {
        Command::Claim { agent, .. }
        | Command::Release { agent, .. }
        | Command::Done { agent }
        | Command::Plan { agent, .. }
        | Command::Heartbeat { agent, .. }
        | Command::Assign { agent, .. } => validate_identifier(agent, "agent ID")?,
        Command::Session {
            action: SessionAction::Start { name },
        } => {
            validate_identifier(name, "session name")?;
        }
        _ => {}
    }

    match cli.command {
        Command::Init => cmd_init(&cli.repo),
        Command::Claim {
            agent,
            intent,
            ttl,
            wait,
            mode,
            queue,
            with_deps,
            symbols,
        } => cmd_claim(
            &cli.repo, &agent, &intent, ttl, wait, &mode, queue, with_deps, &symbols,
        ),
        Command::Release { agent, symbols } => cmd_release(&cli.repo, &agent, &symbols),
        Command::Status => cmd_status(&cli.repo),
        Command::Symbols { file } => cmd_symbols(&cli.repo, file.as_deref()),
        Command::Plan { agent, intent } => cmd_plan(&cli.repo, &agent, &intent),
        Command::Done { agent } => cmd_done(&cli.repo, &agent),
        Command::Watch { poll } => cmd_watch(&cli.repo, poll),
        Command::Worktree { action } => match action {
            WorktreeAction::List => cmd_worktree_list(&cli.repo),
        },
        Command::Queue { action } => match action {
            QueueAction::List => cmd_queue_list(&cli.repo),
            QueueAction::Cancel { agent, symbol } => {
                cmd_queue_cancel(&cli.repo, &agent, symbol.as_deref())
            }
        },
        Command::Gc => cmd_gc(&cli.repo),
        Command::Session { action } => match action {
            SessionAction::Start { name } => cmd_session_start(&cli.repo, &name),
            SessionAction::Status => cmd_session_status(&cli.repo),
            SessionAction::Pr { title } => cmd_session_pr(&cli.repo, title.as_deref()),
            SessionAction::End { name } => cmd_session_end(&cli.repo, name.as_deref()),
        },
        Command::Config { action } => match action {
            ConfigAction::SetS3 {
                bucket,
                endpoint,
                region,
            } => cmd_config_set_s3(&cli.repo, &bucket, endpoint.as_deref(), &region),
            ConfigAction::SetAzure {
                account,
                access_key,
                container,
            } => cmd_config_set_azure(&cli.repo, &account, &access_key, &container),
            ConfigAction::SetLocal => cmd_config_set_local(&cli.repo),
            ConfigAction::Show => cmd_config_show(&cli.repo),
        },
        Command::Heartbeat { agent, ttl } => cmd_heartbeat(&cli.repo, &agent, ttl),
        Command::Assign {
            agent,
            intent,
            file,
            ttl,
            mode,
        } => cmd_assign(&cli.repo, &agent, &intent, &file, ttl, &mode),
    }
}

fn grit_dir(repo: &str) -> std::path::PathBuf {
    std::path::Path::new(repo).join(".grit")
}

/// Ensure .grit directory exists; bail with a clear message otherwise
fn ensure_initialized(repo: &str) -> Result<std::path::PathBuf> {
    let dir = grit_dir(repo);
    if !dir.exists() {
        anyhow::bail!(
            "Not a grit repository (missing {}). Run `grit init` first.",
            dir.display()
        );
    }
    let db_path = dir.join("registry.db");
    if !db_path.exists() {
        anyhow::bail!(
            "grit registry not found (missing {}). Run `grit init` first.",
            db_path.display()
        );
    }
    Ok(dir)
}

/// Resolve the lock store based on config
fn resolve_lock_store(repo: &str) -> Result<Box<dyn LockStore>> {
    let dir = ensure_initialized(repo)?;
    let config = GritConfig::load(&dir)?;

    match config.backend.as_str() {
        "s3" => {
            let s3_config = config.s3.ok_or_else(|| anyhow::anyhow!(
                "S3 backend configured but no S3 config found. Run: grit config set-s3 --bucket <name>"
            ))?;
            let store = crate::db::s3_store::S3LockStore::from_config(&s3_config)?;
            Ok(Box::new(store))
        }
        "azure" => {
            let azure_config = config.azure.ok_or_else(|| anyhow::anyhow!(
                "Azure backend configured but no Azure config found. Run: grit config set-azure --account <name> --access-key <key>"
            ))?;
            let store = crate::db::azure_store::AzureLockStore::from_config(&azure_config)?;
            Ok(Box::new(store))
        }
        _ => {
            let store = SqliteLockStore::open(&dir.join("registry.db"))?;
            Ok(Box::new(store))
        }
    }
}

fn cmd_init(repo: &str) -> Result<()> {
    let dir = grit_dir(repo);
    std::fs::create_dir_all(&dir)?;
    std::fs::create_dir_all(dir.join("worktrees"))?;

    let db = Database::open(&dir.join("registry.db"))?;
    db.init_schema()?;

    // Parse and index all symbols + dependencies
    let index = SymbolIndex::new(repo)?;
    let (symbols, deps) = index.scan_with_deps()?;
    let count = symbols.len();
    let dep_count = deps.len();
    db.upsert_symbols(&symbols)?;
    db.upsert_deps(&deps)?;

    // Add .grit to .gitignore if not already there
    let gitignore = std::path::Path::new(repo).join(".gitignore");
    let should_add = if gitignore.exists() {
        let content = std::fs::read_to_string(&gitignore)?;
        !content.lines().any(|l| l.trim() == ".grit")
    } else {
        true
    };
    if should_add {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gitignore)?;
        writeln!(f, "\n.grit")?;
    }

    // Start notification server
    let server = NotificationServer::new(&dir);
    server.start()?;

    println!("grit initialized");
    println!("  {} symbols indexed", count);
    println!("  {} dependencies found", dep_count);
    println!("  registry: {}", dir.join("registry.db").display());

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_claim(
    repo: &str,
    agent: &str,
    intent: &str,
    ttl: u64,
    wait: u64,
    mode: &str,
    queue: bool,
    with_deps: bool,
    symbols: &[String],
) -> Result<()> {
    if mode != "read" && mode != "write" {
        anyhow::bail!("Invalid mode '{}': must be 'read' or 'write'", mode);
    }
    if symbols.is_empty() {
        anyhow::bail!("No symbols specified to claim (e.g. `grit claim -a <agent> -i <intent> file.rs::symbol`)");
    }

    let dir = ensure_initialized(repo)?;
    let lock_store = resolve_lock_store(repo)?;
    let db = Database::open(&dir.join("registry.db"))?; // for symbol queries

    // If --with-deps, expand symbols to include transitive callees (locked as read)
    let mut dep_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    let symbols = if with_deps {
        let mut expanded = symbols.to_vec();
        for sym_id in symbols {
            if let Ok(deps) = db.get_transitive_deps(sym_id) {
                for dep in deps {
                    if !expanded.contains(&dep) {
                        dep_set.insert(dep.clone());
                        expanded.push(dep);
                    }
                }
            }
        }
        if !dep_set.is_empty() {
            println!(
                "{} Auto-locking {} dependencies as read:",
                "+".cyan(),
                dep_set.len()
            );
            for d in &dep_set {
                println!("  {} {}", ">".cyan(), d);
            }
        }
        expanded
    } else {
        symbols.to_vec()
    };
    let symbols = &symbols;

    let deadline = if wait > 0 {
        Some(std::time::Instant::now() + std::time::Duration::from_secs(wait))
    } else {
        None
    };
    let mut backoff_secs = 1u64;

    loop {
        let mut granted = Vec::new();
        let mut blocked = Vec::new();

        for sym_id in symbols {
            // Deps are always claimed as read locks
            let sym_mode = if dep_set.contains(sym_id) {
                "read"
            } else {
                mode
            };
            match lock_store.try_lock(sym_id, agent, intent, ttl, sym_mode)? {
                LockResult::Granted => granted.push(sym_id.clone()),
                LockResult::Blocked {
                    by_agent,
                    by_intent,
                } => {
                    blocked.push((sym_id.clone(), by_agent, by_intent));
                }
            }
        }

        // If nothing is blocked, or no --wait, or deadline passed: finalize
        let should_retry = !blocked.is_empty()
            && deadline.is_some()
            && std::time::Instant::now() < deadline.unwrap();

        if !should_retry {
            // Create worktree for the agent if any grants succeeded
            if !granted.is_empty() {
                let git_repo = GitRepo::open(repo)?;
                match git_repo.create_worktree(agent) {
                    Ok(wt_path) => {
                        println!("{} Worktree: {}", "+".cyan(), wt_path.display());
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if !msg.contains("already exists") {
                            eprintln!("  warn: could not create worktree: {}", e);
                        }
                    }
                }
            }

            if !granted.is_empty() {
                println!("{} Granted:", "+".green());
                for s in &granted {
                    println!("  {} {}", ">".green(), s);
                }

                let room = Room::new(&dir);
                room.notify(&RoomEvent {
                    event_type: EventType::Claimed,
                    agent: agent.to_string(),
                    symbols: granted,
                });
            }

            if !blocked.is_empty() {
                if queue {
                    // Enqueue blocked symbols
                    for (s, by, by_intent) in &blocked {
                        db.enqueue(s, agent, intent, mode)?;
                        let pos = db.queue_position(s, agent)?.unwrap_or(0);
                        println!(
                            "{} Queued: {} (position {}, held by {} - {})",
                            "~".yellow(),
                            s,
                            pos,
                            by,
                            by_intent
                        );
                    }
                } else {
                    println!("{} Blocked:", "x".red());
                    for (s, by, intent) in &blocked {
                        println!("  {} {} -- held by {} ({})", ">".red(), s, by, intent);
                    }
                }

                let files: Vec<&str> = symbols
                    .iter()
                    .filter_map(|s| s.split("::").next())
                    .collect();
                let available = db.available_symbols_in_files(&files)?;
                if !available.is_empty() {
                    println!("\n{} Available in same files:", "?".yellow());
                    for s in &available {
                        println!("  {} {}", ">".yellow(), s);
                    }
                }
            }

            return if blocked.is_empty() || queue {
                Ok(())
            } else {
                anyhow::bail!("Some symbols are blocked")
            };
        }

        // Retrying: print waiting message for each blocked symbol
        for (s, by, _intent) in &blocked {
            println!("Waiting for {} (held by {})...", s, by);
        }

        // Release symbols we just locked so they aren't held during sleep
        // (all-or-nothing: either all symbols are claimed or none)
        for s in &granted {
            let _ = lock_store.release(s, agent);
        }

        // Sleep with backoff: 1s -> 2s -> 4s -> 5s (cap), never past deadline
        let remaining = deadline
            .unwrap()
            .saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            continue; // one final attempt
        }
        let sleep_dur = std::time::Duration::from_secs(backoff_secs).min(remaining);
        std::thread::sleep(sleep_dur);
        backoff_secs = (backoff_secs * 2).min(5);
    }
}

fn cmd_release(repo: &str, agent: &str, symbols: &[String]) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let lock_store = resolve_lock_store(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;

    let released_symbols = if symbols.is_empty() {
        // Get all symbols held by this agent before releasing (for queue promotion)
        let held = lock_store.locks_for_agent(agent)?;
        let syms: Vec<String> = held.iter().map(|(s, _)| s.clone()).collect();
        let released = lock_store.release_all(agent)?;
        println!("Released {} symbols for {}", released, agent);
        syms
    } else {
        for sym_id in symbols {
            lock_store.release(sym_id, agent)?;
            println!("Released {}", sym_id);
        }
        symbols.to_vec()
    };

    // Promote queued agents for released symbols
    promote_queued(&db, lock_store.as_ref(), &released_symbols, &dir)?;

    let room = Room::new(&dir);
    room.notify(&RoomEvent {
        event_type: EventType::Released,
        agent: agent.to_string(),
        symbols: if symbols.is_empty() {
            vec!["(all)".to_string()]
        } else {
            symbols.to_vec()
        },
    });

    Ok(())
}

/// Promote the next queued agent for each released symbol
fn promote_queued(
    db: &Database,
    lock_store: &dyn LockStore,
    symbols: &[String],
    grit_dir: &std::path::Path,
) -> Result<()> {
    let room = Room::new(grit_dir);
    for sym_id in symbols {
        // Drain the queue head for this symbol. A granted WRITE lock is
        // exclusive, so stop after it. A granted READ lock is shared, so keep
        // draining consecutive queued readers until the head is a writer (which
        // will block on the readers we just granted) or the queue empties.
        // TODO(#queue-ttl-worktree): promotion uses a hardcoded 600s TTL and
        // does not create the promoted agent's worktree — both require storing
        // the request TTL in lock_queue and threading GitRepo here.
        loop {
            let Some((next_agent, next_intent, next_mode)) = db.next_in_queue(sym_id)? else {
                break;
            };
            match lock_store.try_lock(sym_id, &next_agent, &next_intent, 600, &next_mode)? {
                LockResult::Granted => {
                    db.dequeue(sym_id, &next_agent)?;
                    println!(
                        "{} Auto-granted {} to {} (from queue)",
                        "+".green(),
                        sym_id,
                        next_agent
                    );
                    room.notify(&RoomEvent {
                        event_type: EventType::Claimed,
                        agent: next_agent,
                        symbols: vec![sym_id.clone()],
                    });
                    // Only keep draining if this was a shared read lock.
                    if next_mode != "read" {
                        break;
                    }
                }
                LockResult::Blocked { .. } => {
                    // Head can't be granted yet (e.g. a writer behind readers),
                    // leave it and the rest of the queue in place.
                    break;
                }
            }
        }
    }
    Ok(())
}

fn cmd_status(repo: &str) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let lock_store = resolve_lock_store(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;

    let locks = lock_store.all_locks()?;

    if locks.is_empty() {
        println!("No active locks.");
        return Ok(());
    }

    // Group by agent
    let mut by_agent: std::collections::BTreeMap<String, Vec<&LockEntry>> =
        std::collections::BTreeMap::new();
    for entry in &locks {
        by_agent
            .entry(entry.agent_id.clone())
            .or_default()
            .push(entry);
    }

    for (agent, entries) in &by_agent {
        let intent = &entries[0].intent;
        println!("{} {} -- {}", "*".green(), agent.bold(), intent.dimmed());
        for entry in entries {
            // Compute expiry locally from the LockEntry data we already have,
            // instead of making another round-trip per lock (N+1 on S3 backend)
            let expired = is_entry_expired_local(entry);
            let status = if expired {
                "EXPIRED".red().to_string()
            } else {
                format!("ttl={}s", entry.ttl_seconds)
            };
            let mode_str = if entry.mode == "read" { " (read)" } else { "" };
            println!(
                "  {} {}{} ({}) [{}]",
                "|".dimmed(),
                entry.symbol_id,
                mode_str,
                entry.locked_at.dimmed(),
                status
            );
        }
    }

    let total_symbols = db.count_symbols()?;
    let locked_count = locks.len();
    let queue_count = db.list_queue()?.len();
    println!(
        "\n{}/{} symbols locked{}",
        locked_count,
        total_symbols,
        if queue_count > 0 {
            format!(", {} queued", queue_count)
        } else {
            String::new()
        }
    );

    Ok(())
}

fn cmd_symbols(repo: &str, file_filter: Option<&str>) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;

    let symbols = db.list_symbols(file_filter)?;

    if symbols.is_empty() {
        println!("No symbols found. Run `grit init` first.");
        return Ok(());
    }

    let mut current_file = String::new();
    for (_id, file, name, kind, locked_by) in &symbols {
        if file != &current_file {
            current_file = file.clone();
            println!("\n{}", file.bold());
        }
        let lock_indicator = match locked_by {
            Some(agent) => format!(" [locked: {}]", agent.red()),
            None => String::new(),
        };
        println!(
            "  {} {} ({}){}",
            "|".dimmed(),
            name,
            kind.dimmed(),
            lock_indicator
        );
    }

    Ok(())
}

fn cmd_plan(repo: &str, agent: &str, intent: &str) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;

    // Search symbols related to the intent keywords
    let keywords: Vec<&str> = intent.split_whitespace().collect();
    let suggestions = db.search_symbols(&keywords)?;

    println!("Planning for: {}", intent.bold());
    println!("\nRelevant symbols:");

    for (_id, file, name, kind, locked_by) in &suggestions {
        let status = match locked_by {
            Some(agent) => format!("{} ({})", "LOCKED".red(), agent),
            None => "FREE".green().to_string(),
        };
        println!(
            "  {} {}::{} [{}] {}",
            ">".dimmed(),
            file,
            name,
            kind,
            status
        );
    }

    // Show dependencies for each symbol
    for (id, _, _, _, _) in &suggestions {
        let deps = db.get_transitive_deps(id)?;
        if !deps.is_empty() {
            println!("  {} deps: {}", "|".dimmed(), deps.join(", "));
        }
    }

    let free: Vec<&String> = suggestions
        .iter()
        .filter(|(_, _, _, _, l)| l.is_none())
        .map(|(id, _, _, _, _)| id)
        .collect();

    if !free.is_empty() {
        println!(
            "\nClaim with:\n  grit claim -a {} -i \"{}\" {}",
            agent,
            intent,
            free.iter()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(" ")
        );
        println!(
            "\nClaim with deps:\n  grit claim -a {} -i \"{}\" --with-deps {}",
            agent,
            intent,
            free.iter()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    Ok(())
}

fn cmd_done(repo: &str, agent: &str) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let lock_store = resolve_lock_store(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;

    let locks = lock_store.locks_for_agent(agent)?;
    if locks.is_empty() {
        println!("Agent {} has no active locks.", agent);
        return Ok(());
    }

    println!("{} Agent {} finishing:", "+".green(), agent.bold());
    for (sym, _intent) in &locks {
        println!("  {} releasing {}", ">".dimmed(), sym);
    }

    // Always release locks even if merge/cleanup fails,
    // to prevent orphan locks when the process crashes mid-operation.
    let git_repo = GitRepo::open(repo)?;
    let mut merge_error: Option<String> = None;
    let mut merged = false;

    // Try to merge worktree back
    match git_repo.merge_worktree(agent) {
        Ok(()) => {
            println!("{} Merged branch agent/{}", "+".green(), agent);
            merged = true;
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("does not exist") {
                // No worktree to merge — nothing to clean up either.
            } else {
                merge_error = Some(msg);
            }
        }
    }

    // Only tear down the worktree and delete the agent branch once the merge
    // succeeded. If the merge was skipped or failed, keep both so the agent's
    // commit stays reachable and recoverable (issue #21).
    if merged {
        match git_repo.remove_worktree(agent) {
            Ok(()) => {
                println!("{} Removed worktree for {}", "+".green(), agent);
            }
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("not found") && !msg.contains("does not exist") {
                    eprintln!("  warn: could not remove worktree: {}", e);
                }
            }
        }
        let _ = git_repo.delete_agent_branch(agent);
    } else if merge_error.is_some() {
        eprintln!(
            "  warn: merge skipped — keeping worktree .grit/worktrees/{} and branch agent/{} for recovery",
            agent, agent
        );
    }

    // Release locks regardless of merge outcome
    let released_symbols: Vec<String> = locks.iter().map(|(s, _)| s.clone()).collect();
    let released = lock_store.release_all(agent)?;
    println!("{} Released {} symbols", "+".green(), released);

    // Also clear any queue entries for this agent
    let dequeued = db.dequeue_all(agent)?;
    if dequeued > 0 {
        println!("{} Removed {} queue entries", "+".green(), dequeued);
    }

    // Promote queued agents for released symbols
    promote_queued(&db, lock_store.as_ref(), &released_symbols, &dir)?;

    // Notify
    let room = Room::new(&dir);
    room.notify(&RoomEvent {
        event_type: EventType::AgentDone,
        agent: agent.to_string(),
        symbols: released_symbols,
    });

    // Report merge failure after cleanup is complete
    if let Some(err) = merge_error {
        anyhow::bail!(
            "Agent {} locks released but merge failed: {}.\n\
             The agent's changes are still in branch agent/{}.",
            agent,
            err,
            agent
        );
    }

    Ok(())
}

fn cmd_watch(repo: &str, poll: Option<u64>) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let config = GritConfig::load(&dir)?;

    // For S3 backend or explicit --poll, use polling mode
    if poll.is_some() || config.backend == "s3" {
        return cmd_watch_poll(repo, poll.unwrap_or(5));
    }

    let sock_path = dir.join("room.sock");

    if !sock_path.exists() {
        anyhow::bail!(
            "No room socket found at {}.\n\
             The notification server only runs during `grit init`.\n\
             Re-run `grit init` in a long-lived process, or use `grit status` to poll.",
            sock_path.display()
        );
    }

    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixStream;

    println!("Connecting to room socket at {}...", sock_path.display());
    let stream = match UnixStream::connect(&sock_path) {
        Ok(s) => s,
        Err(e) => {
            // Socket file exists but server is dead -- clean up stale socket
            if e.kind() == std::io::ErrorKind::ConnectionRefused {
                let _ = std::fs::remove_file(&sock_path);
                anyhow::bail!(
                    "Room socket is stale (server not running). Removed {}.\n\
                     Re-run `grit init` to start the notification server.",
                    sock_path.display()
                );
            }
            return Err(e.into());
        }
    };
    let reader = BufReader::new(stream);

    println!("Watching for events (Ctrl+C to stop):\n");

    for line in reader.lines() {
        match line {
            Ok(data) => {
                if data.is_empty() {
                    continue;
                }
                match serde_json::from_str::<RoomEvent>(&data) {
                    Ok(event) => {
                        print_event(&event);
                    }
                    Err(_) => {
                        println!("  raw: {}", data);
                    }
                }
            }
            Err(e) => {
                eprintln!("Socket read error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

fn print_event(event: &RoomEvent) {
    let prefix = match event.event_type {
        EventType::Claimed => "CLAIMED".green(),
        EventType::Released => "RELEASED".yellow(),
        EventType::AgentDone => "DONE".cyan(),
    };
    println!(
        "[{}] agent={} symbols=[{}]",
        prefix,
        event.agent,
        event.symbols.join(", ")
    );
}

/// Poll-based watch: periodically diff the lock state and report changes
fn cmd_watch_poll(repo: &str, interval_secs: u64) -> Result<()> {
    let lock_store = resolve_lock_store(repo)?;

    println!(
        "Polling for lock changes every {}s (Ctrl+C to stop):\n",
        interval_secs
    );

    // Build initial snapshot: symbol_id -> (agent_id, intent)
    let mut prev: std::collections::HashMap<String, (String, String)> = lock_store
        .all_locks()?
        .into_iter()
        .map(|e| (e.symbol_id.clone(), (e.agent_id.clone(), e.intent.clone())))
        .collect();

    loop {
        std::thread::sleep(std::time::Duration::from_secs(interval_secs));

        let current_locks = lock_store.all_locks()?;
        let curr: std::collections::HashMap<String, (String, String)> = current_locks
            .iter()
            .map(|e| (e.symbol_id.clone(), (e.agent_id.clone(), e.intent.clone())))
            .collect();

        // Detect new locks (claimed)
        for (sym, (agent, _intent)) in &curr {
            if !prev.contains_key(sym) {
                print_event(&RoomEvent {
                    event_type: EventType::Claimed,
                    agent: agent.clone(),
                    symbols: vec![sym.clone()],
                });
            }
        }

        // Detect removed locks (released)
        for (sym, (agent, _intent)) in &prev {
            if !curr.contains_key(sym) {
                print_event(&RoomEvent {
                    event_type: EventType::Released,
                    agent: agent.clone(),
                    symbols: vec![sym.clone()],
                });
            }
        }

        prev = curr;
    }
}

fn cmd_worktree_list(repo: &str) -> Result<()> {
    let git_repo = GitRepo::open(repo)?;
    let worktrees = git_repo.list_worktrees()?;

    if worktrees.is_empty() {
        println!("No active worktrees.");
        return Ok(());
    }

    println!("{}", "Active worktrees:".bold());
    for agent_id in &worktrees {
        let dir = grit_dir(repo).join("worktrees").join(agent_id);
        println!("  {} {} -> {}", ">".green(), agent_id, dir.display());
    }

    Ok(())
}

fn cmd_queue_list(repo: &str) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;

    let entries = db.list_queue()?;
    if entries.is_empty() {
        println!("No agents in queue.");
        return Ok(());
    }

    let mut current_symbol = String::new();
    let mut pos = 0;
    for (symbol_id, agent_id, intent, mode, queued_at) in &entries {
        if symbol_id != &current_symbol {
            current_symbol = symbol_id.clone();
            pos = 0;
            println!("\n{}", symbol_id.bold());
        }
        pos += 1;
        let mode_str = if mode == "read" { " (read)" } else { "" };
        println!(
            "  {} #{} {} -- {}{} ({})",
            ">".yellow(),
            pos,
            agent_id,
            intent,
            mode_str,
            queued_at.dimmed()
        );
    }

    Ok(())
}

fn cmd_queue_cancel(repo: &str, agent: &str, symbol: Option<&str>) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;

    match symbol {
        Some(sym) => {
            db.dequeue(sym, agent)?;
            println!("Removed {} from queue for {}", agent, sym);
        }
        None => {
            let count = db.dequeue_all(agent)?;
            println!("Removed {} queue entries for {}", count, agent);
        }
    }

    Ok(())
}

fn cmd_gc(repo: &str) -> Result<()> {
    let lock_store = resolve_lock_store(repo)?;

    let expired = lock_store.gc_expired_locks()?;
    if expired == 0 {
        println!("No expired locks found.");
    } else {
        println!("Cleaned up {} expired locks.", expired);
    }

    Ok(())
}

fn cmd_assign(
    repo: &str,
    agent: &str,
    intent: &str,
    file_pattern: &str,
    ttl: u64,
    mode: &str,
) -> Result<()> {
    if mode != "read" && mode != "write" {
        anyhow::bail!("Invalid mode '{}': must be 'read' or 'write'", mode);
    }

    let dir = ensure_initialized(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;
    let lock_store = resolve_lock_store(repo)?;

    // Find matching files
    let all_symbols = db.list_symbols(Some(file_pattern))?;
    if all_symbols.is_empty() {
        anyhow::bail!("No symbols found matching file pattern '{}'", file_pattern);
    }

    // Get unique files
    let files: Vec<&str> = all_symbols
        .iter()
        .map(|(_, f, _, _, _)| f.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let available = db.available_symbols_in_files(&files)?;

    if available.is_empty() {
        anyhow::bail!("All symbols in matching files are locked. Try again later or use a different file pattern.");
    }

    // Pick the first available symbol and claim it
    let symbol_id = &available[0];
    match lock_store.try_lock(symbol_id, agent, intent, ttl, mode)? {
        LockResult::Granted => {
            // Create worktree
            let git_repo = GitRepo::open(repo)?;
            match git_repo.create_worktree(agent) {
                Ok(wt_path) => {
                    println!("{} Worktree: {}", "+".cyan(), wt_path.display());
                }
                Err(e) => {
                    let msg = e.to_string();
                    if !msg.contains("already exists") {
                        eprintln!("  warn: could not create worktree: {}", e);
                    }
                }
            }

            println!("{} Assigned: {}", "+".green(), symbol_id);
            println!("  agent:  {}", agent);
            println!("  intent: {}", intent);
            println!("  mode:   {}", mode);

            let room = Room::new(&dir);
            room.notify(&RoomEvent {
                event_type: EventType::Claimed,
                agent: agent.to_string(),
                symbols: vec![symbol_id.clone()],
            });

            Ok(())
        }
        LockResult::Blocked {
            by_agent,
            by_intent,
        } => {
            // Race condition — symbol was claimed between availability check and lock attempt
            anyhow::bail!(
                "Symbol {} was claimed by {} ({}) between check and lock. Try again.",
                symbol_id,
                by_agent,
                by_intent
            );
        }
    }
}

fn cmd_heartbeat(repo: &str, agent: &str, ttl: u64) -> Result<()> {
    let lock_store = resolve_lock_store(repo)?;

    let refreshed = lock_store.refresh_ttl(agent, ttl)?;
    if refreshed == 0 {
        println!("Agent {} has no active locks to refresh.", agent);
    } else {
        println!("Refreshed TTL for {} locks (new ttl={}s).", refreshed, ttl);
    }

    Ok(())
}

// ── Session commands ──

fn cmd_session_start(repo: &str, name: &str) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;
    let git_repo = GitRepo::open(repo)?;

    let base_branch = git_repo.current_branch()?;
    let branch = git_repo.create_session_branch(name)?;
    db.create_session(name, &branch, &base_branch)?;

    println!("{} Session started: {}", "+".green(), name.bold());
    println!("  branch: {}", branch.cyan());
    println!("  base:   {}", base_branch.dimmed());
    println!();
    println!("Agents can now work:");
    println!("  grit claim -a agent-1 -i \"task\" <symbols...>");
    println!("  # edit in .grit/worktrees/agent-1/");
    println!("  grit done -a agent-1");
    println!();
    println!("When all agents are done:");
    println!("  grit session pr");

    Ok(())
}

fn cmd_session_status(repo: &str) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;
    let lock_store = resolve_lock_store(repo)?;

    match db.get_active_session()? {
        Some((name, branch, base)) => {
            println!("{} Active session: {}", "*".green(), name.bold());
            println!("  branch: {}", branch.cyan());
            println!("  base:   {}", base.dimmed());

            let locks = lock_store.all_locks()?;
            let git_repo = GitRepo::open(repo)?;
            let worktrees = git_repo.list_worktrees()?;

            println!("  agents: {} active worktrees", worktrees.len());
            println!("  locks:  {} symbols locked", locks.len());

            if !worktrees.is_empty() {
                println!("\n  Active agents:");
                for wt in &worktrees {
                    println!("    {} {}", ">".green(), wt);
                }
            }
        }
        None => {
            println!("No active session.");
            println!("Start one with: grit session start <name>");
        }
    }

    Ok(())
}

fn cmd_session_pr(repo: &str, title: Option<&str>) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;
    let git_repo = GitRepo::open(repo)?;

    let (name, branch, base) = db.get_active_session()?.ok_or_else(|| {
        anyhow::anyhow!("No active session. Start one with: grit session start <name>")
    })?;

    // Check for remaining locks
    let lock_store = resolve_lock_store(repo)?;
    let locks = lock_store.all_locks()?;
    let worktrees = git_repo.list_worktrees()?;

    if !worktrees.is_empty() {
        println!(
            "{} Warning: {} agents still have active worktrees:",
            "!".yellow(),
            worktrees.len()
        );
        for wt in &worktrees {
            println!("  {} {}", ">".yellow(), wt);
        }
        println!("Run 'grit done -a <agent>' for each, or proceed anyway.\n");
    }

    if !locks.is_empty() {
        println!(
            "{} Warning: {} symbols still locked",
            "!".yellow(),
            locks.len()
        );
    }

    let pr_title = title.unwrap_or(&name);

    // Build PR body with session summary
    let total_symbols = db.count_symbols()?;
    let body = format!(
        "## Summary\n\
         Multi-agent session `{}` coordinated by grit.\n\n\
         - **Branch**: `{}` -> `{}`\n\
         - **Symbols indexed**: {}\n\
         - **Remaining locks**: {}\n\n\
         ## Agent Activity\n\
         Agents worked in isolated git worktrees with AST-level symbol locking.\n\
         Zero merge conflicts by design.\n\n\
         ---\n\
         *Coordinated by [grit](https://github.com/pszymkowiak/grit)*",
        name,
        branch,
        base,
        total_symbols,
        locks.len()
    );

    println!("Creating PR: {} -> {}", branch.cyan(), base.dimmed());
    let pr_url = git_repo.push_and_create_pr(&branch, pr_title, &body)?;

    println!("{} PR created: {}", "+".green(), pr_url);

    Ok(())
}

fn cmd_session_end(repo: &str, _name: Option<&str>) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let db = Database::open(&dir.join("registry.db"))?;
    let lock_store = resolve_lock_store(repo)?;
    let git_repo = GitRepo::open(repo)?;

    let (session_name, _branch, base) = db
        .get_active_session()?
        .ok_or_else(|| anyhow::anyhow!("No active session"))?;

    // GC any expired locks
    let expired = lock_store.gc_expired_locks()?;
    if expired > 0 {
        println!("  Cleaned up {} expired locks", expired);
    }

    // Close session in DB
    db.close_session(&session_name)?;

    // Switch back to base branch
    git_repo.checkout(&base)?;

    println!("{} Session '{}' ended", "+".green(), session_name.bold());
    println!("  Switched back to {}", base.cyan());

    Ok(())
}

// ── Config commands ──

fn cmd_config_set_s3(repo: &str, bucket: &str, endpoint: Option<&str>, region: &str) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let config = GritConfig {
        backend: "s3".to_string(),
        s3: Some(S3Config {
            bucket: bucket.to_string(),
            endpoint: endpoint.map(|s| s.to_string()),
            region: Some(region.to_string()),
            prefix: None,
        }),
        azure: None,
    };
    config.save(&dir)?;

    println!("{} Backend set to S3", "+".green());
    println!("  bucket:   {}", bucket.cyan());
    if let Some(ep) = endpoint {
        println!("  endpoint: {}", ep.cyan());
    }
    println!("  region:   {}", region);
    println!();
    println!("Compatible with: AWS S3, Cloudflare R2, GCS, Azure Blob, MinIO");
    println!();
    println!("Set credentials via environment:");
    println!("  export AWS_ACCESS_KEY_ID=...");
    println!("  export AWS_SECRET_ACCESS_KEY=...");

    Ok(())
}

fn cmd_config_set_azure(
    repo: &str,
    account: &str,
    access_key: &str,
    container: &str,
) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let config = GritConfig {
        backend: "azure".to_string(),
        s3: None,
        azure: Some(AzureConfig {
            account: account.to_string(),
            access_key: access_key.to_string(),
            container: container.to_string(),
            prefix: None,
        }),
    };
    config.save(&dir)?;

    println!("{} Backend set to Azure Blob Storage", "+".green());
    println!("  account:   {}", account.cyan());
    println!("  container: {}", container);
    println!();
    println!("Events: Azure Event Grid fires on every claim/release (free tier: 100K/mo)");

    Ok(())
}

fn cmd_config_set_local(repo: &str) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let config = GritConfig {
        backend: "local".to_string(),
        s3: None,
        azure: None,
    };
    config.save(&dir)?;

    println!("{} Backend set to local (SQLite)", "+".green());

    Ok(())
}

fn cmd_config_show(repo: &str) -> Result<()> {
    let dir = ensure_initialized(repo)?;
    let config = GritConfig::load(&dir)?;

    println!("{} Current config:", "*".green());
    println!("  backend: {}", config.backend.cyan());

    if let Some(ref s3) = config.s3 {
        println!("  s3.bucket:   {}", s3.bucket);
        if let Some(ref ep) = s3.endpoint {
            println!("  s3.endpoint: {}", ep);
        }
        if let Some(ref r) = s3.region {
            println!("  s3.region:   {}", r);
        }
    }
    if let Some(ref az) = config.azure {
        println!("  azure.account:   {}", az.account);
        println!("  azure.container: {}", az.container);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::lock_store::LockEntry;

    // ── validate_identifier tests ──

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("agent-1", "id").is_ok());
        assert!(validate_identifier("my_agent", "id").is_ok());
        assert!(validate_identifier("agent.v2", "id").is_ok());
        assert!(validate_identifier("abc123", "id").is_ok());
    }

    #[test]
    fn test_validate_identifier_empty() {
        assert!(validate_identifier("", "id").is_err());
    }

    #[test]
    fn test_validate_identifier_path_traversal() {
        assert!(validate_identifier("..", "id").is_err());
    }

    #[test]
    fn test_validate_identifier_slash() {
        assert!(validate_identifier("foo/bar", "id").is_err());
    }

    #[test]
    fn test_validate_identifier_backslash() {
        assert!(validate_identifier("foo\\bar", "id").is_err());
    }

    #[test]
    fn test_validate_identifier_starts_with_dash() {
        assert!(validate_identifier("-agent", "id").is_err());
    }

    #[test]
    fn test_validate_identifier_dot_edges() {
        assert!(validate_identifier(".", "id").is_err());
        assert!(validate_identifier(".agent", "id").is_err());
        assert!(validate_identifier("agent.", "id").is_err());
    }

    #[test]
    fn test_validate_identifier_special_chars() {
        assert!(validate_identifier("foo@bar", "id").is_err());
        assert!(validate_identifier("foo bar", "id").is_err());
        assert!(validate_identifier("foo;rm", "id").is_err());
    }

    // ── is_entry_expired_local tests ──

    fn make_entry(locked_at: &str, ttl: u64) -> LockEntry {
        LockEntry {
            symbol_id: "test::sym".to_string(),
            agent_id: "agent-1".to_string(),
            intent: "testing".to_string(),
            locked_at: locked_at.to_string(),
            ttl_seconds: ttl,
            mode: "write".to_string(),
        }
    }

    #[test]
    fn test_is_entry_expired_local_fresh() {
        let now = chrono::Utc::now().to_rfc3339();
        let entry = make_entry(&now, 600);
        assert!(!is_entry_expired_local(&entry));
    }

    #[test]
    fn test_is_entry_expired_local_expired() {
        let one_hour_ago = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let entry = make_entry(&one_hour_ago, 60);
        assert!(is_entry_expired_local(&entry));
    }

    #[test]
    fn test_is_entry_expired_local_bad_timestamp() {
        let entry = make_entry("not-a-timestamp", 600);
        assert!(is_entry_expired_local(&entry));
    }

    // ── claim wait/backoff tests ──

    #[test]
    fn test_claim_wait_backoff_gives_up_after_timeout() {
        // Set up a temp repo with grit init
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().to_str().unwrap();

        // Init git repo
        std::process::Command::new("git")
            .args(["init", repo])
            .output()
            .unwrap();
        std::fs::write(
            tmp.path().join("main.rs"),
            "fn hello() {}
",
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["-C", repo, "add", "."])
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C", repo, "commit", "-m", "init"])
            .output()
            .unwrap();

        // Init grit
        cmd_init(repo).unwrap();

        // Blocker agent claims a symbol
        let lock_store = resolve_lock_store(repo).unwrap();
        let result = lock_store
            .try_lock("main.rs::hello", "blocker", "blocking", 600, "write")
            .unwrap();
        assert!(matches!(result, LockResult::Granted));

        // Agent-2 tries to claim with --wait 2 (should timeout after ~2s)
        let start = std::time::Instant::now();
        let err = cmd_claim(
            repo,
            "agent-2",
            "want it",
            600,
            2,
            "write",
            false,
            false,
            &["main.rs::hello".to_string()],
        );
        let elapsed = start.elapsed();

        // Should have failed (blocked)
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("blocked"),
            "Expected 'blocked' in error: {}",
            msg
        );

        // Should have waited at least 1s (first backoff) but not more than 4s
        assert!(
            elapsed.as_secs() >= 1,
            "Should have retried at least once, elapsed: {:?}",
            elapsed
        );
        assert!(
            elapsed.as_secs() <= 4,
            "Should not wait too long, elapsed: {:?}",
            elapsed
        );
    }
}
