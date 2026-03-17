use anyhow::Result;
use clap::{Parser, Subcommand};

use colored::Colorize;

use crate::config::GritConfig;
use crate::db::Database;
use crate::db::lock_store::{LockEntry, LockStore, LockResult};
use crate::db::sqlite_store::SqliteLockStore;
use crate::db::s3_store::S3Config;
use crate::git::GitRepo;
use crate::parser::SymbolIndex;
use crate::room::{Room, RoomEvent, EventType, NotificationServer};

#[derive(Parser)]
#[command(name = "grit", about = "Coordination layer for parallel AI agents on top of git")]
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

    /// Watch real-time events from the room socket
    Watch,

    /// Manage git worktrees
    Worktree {
        #[command(subcommand)]
        action: WorktreeAction,
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
    /// Set backend to local SQLite (default)
    SetLocal,
    /// Show current config
    Show,
}

/// Validate agent/session identifiers to prevent path traversal and argument injection
fn validate_identifier(id: &str, label: &str) -> Result<()> {
    if id.is_empty() {
        anyhow::bail!("Invalid {}: must not be empty", label);
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") || id.starts_with('-') {
        anyhow::bail!("Invalid {}: '{}' contains forbidden characters (/, \\, ..) or starts with -", label, id);
    }
    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
        anyhow::bail!("Invalid {}: '{}' must contain only alphanumeric, hyphens, underscores, dots", label, id);
    }
    Ok(())
}

pub fn run(cli: Cli) -> Result<()> {
    // Validate identifiers early to prevent path traversal / argument injection
    match &cli.command {
        Command::Claim { agent, .. } | Command::Release { agent, .. }
        | Command::Done { agent } | Command::Plan { agent, .. }
        | Command::Heartbeat { agent, .. } => validate_identifier(agent, "agent ID")?,
        Command::Session { action } => {
            if let SessionAction::Start { name } = action {
                validate_identifier(name, "session name")?;
            }
        }
        _ => {}
    }

    match cli.command {
        Command::Init => cmd_init(&cli.repo),
        Command::Claim { agent, intent, ttl, symbols } => cmd_claim(&cli.repo, &agent, &intent, ttl, &symbols),
        Command::Release { agent, symbols } => cmd_release(&cli.repo, &agent, &symbols),
        Command::Status => cmd_status(&cli.repo),
        Command::Symbols { file } => cmd_symbols(&cli.repo, file.as_deref()),
        Command::Plan { agent, intent } => cmd_plan(&cli.repo, &agent, &intent),
        Command::Done { agent } => cmd_done(&cli.repo, &agent),
        Command::Watch => cmd_watch(&cli.repo),
        Command::Worktree { action } => match action {
            WorktreeAction::List => cmd_worktree_list(&cli.repo),
        },
        Command::Gc => cmd_gc(&cli.repo),
        Command::Session { action } => match action {
            SessionAction::Start { name } => cmd_session_start(&cli.repo, &name),
            SessionAction::Status => cmd_session_status(&cli.repo),
            SessionAction::Pr { title } => cmd_session_pr(&cli.repo, title.as_deref()),
            SessionAction::End { name } => cmd_session_end(&cli.repo, name.as_deref()),
        },
        Command::Config { action } => match action {
            ConfigAction::SetS3 { bucket, endpoint, region } => cmd_config_set_s3(&cli.repo, &bucket, endpoint.as_deref(), &region),
            ConfigAction::SetLocal => cmd_config_set_local(&cli.repo),
            ConfigAction::Show => cmd_config_show(&cli.repo),
        },
        Command::Heartbeat { agent, ttl } => cmd_heartbeat(&cli.repo, &agent, ttl),
    }
}

fn grit_dir(repo: &str) -> std::path::PathBuf {
    std::path::Path::new(repo).join(".grit")
}

/// Resolve the lock store based on config
fn resolve_lock_store(repo: &str) -> Result<Box<dyn LockStore>> {
    let dir = grit_dir(repo);
    let config = GritConfig::load(&dir)?;

    match config.backend.as_str() {
        "s3" => {
            let s3_config = config.s3.ok_or_else(|| anyhow::anyhow!(
                "S3 backend configured but no S3 config found. Run: grit config set-s3 --bucket <name>"
            ))?;
            let store = crate::db::s3_store::S3LockStore::from_config(&s3_config)?;
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

    // Parse and index all symbols
    let index = SymbolIndex::new(repo)?;
    let symbols = index.scan_all()?;
    let count = symbols.len();
    db.upsert_symbols(&symbols)?;

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
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&gitignore)?;
        writeln!(f, "\n.grit")?;
    }

    // Start notification server
    let server = NotificationServer::new(&dir);
    server.start()?;

    println!("grit initialized");
    println!("  {} symbols indexed", count);
    println!("  registry: {}", dir.join("registry.db").display());

    Ok(())
}

fn cmd_claim(repo: &str, agent: &str, intent: &str, ttl: u64, symbols: &[String]) -> Result<()> {
    let dir = grit_dir(repo);
    let lock_store = resolve_lock_store(repo)?;
    let db = Database::open(&dir.join("registry.db"))?; // for symbol queries

    let mut granted = Vec::new();
    let mut blocked = Vec::new();

    for sym_id in symbols {
        match lock_store.try_lock(sym_id, agent, intent, ttl)? {
            LockResult::Granted => granted.push(sym_id.clone()),
            LockResult::Blocked { by_agent, by_intent } => {
                blocked.push((sym_id.clone(), by_agent, by_intent));
            }
        }
    }

    // Create worktree for the agent if any grants succeeded
    if !granted.is_empty() {
        let git_repo = GitRepo::open(repo)?;
        match git_repo.create_worktree(agent) {
            Ok(wt_path) => {

                println!("{} Worktree: {}", "+".cyan(), wt_path.display());
            }
            Err(e) => {
                // Worktree may already exist, that's fine
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

        // Notify
        let room = Room::new(&dir);
        room.notify(&RoomEvent {
            event_type: EventType::Claimed,
            agent: agent.to_string(),
            symbols: granted.clone(),
        });
    }

    if !blocked.is_empty() {

        println!("{} Blocked:", "x".red());
        for (s, by, intent) in &blocked {
            println!("  {} {} -- held by {} ({})", ">".red(), s, by, intent);
        }

        // Suggest available symbols in the same files
        let files: Vec<&str> = symbols.iter().filter_map(|s| s.split("::").next()).collect();
        let available = db.available_symbols_in_files(&files)?;
        if !available.is_empty() {
            println!("\n{} Available in same files:", "?".yellow());
            for s in &available {
                println!("  {} {}", ">".yellow(), s);
            }
        }
    }

    if blocked.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("Some symbols are blocked")
    }
}

fn cmd_release(repo: &str, agent: &str, symbols: &[String]) -> Result<()> {
    let dir = grit_dir(repo);
    let lock_store = resolve_lock_store(repo)?;

    if symbols.is_empty() {
        let released = lock_store.release_all(agent)?;
        println!("Released {} symbols for {}", released, agent);
    } else {
        for sym_id in symbols {
            lock_store.release(sym_id, agent)?;
            println!("Released {}", sym_id);
        }
    }

    let room = Room::new(&dir);
    let released_symbols = if symbols.is_empty() {
        vec!["(all)".to_string()]
    } else {
        symbols.to_vec()
    };
    room.notify(&RoomEvent {
        event_type: EventType::Released,
        agent: agent.to_string(),
        symbols: released_symbols,
    });

    Ok(())
}

fn cmd_status(repo: &str) -> Result<()> {
    let dir = grit_dir(repo);
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
        by_agent.entry(entry.agent_id.clone()).or_default().push(entry);
    }

    for (agent, entries) in &by_agent {
        let intent = &entries[0].intent;
        println!("{} {} -- {}", "*".green(), agent.bold(), intent.dimmed());
        for entry in entries {
            let expired = lock_store.is_lock_expired(&entry.symbol_id).unwrap_or(false);
            let status = if expired {
                "EXPIRED".red().to_string()
            } else {
                format!("ttl={}s", entry.ttl_seconds)
            };
            println!("  {} {} ({}) [{}]", "|".dimmed(), entry.symbol_id, entry.locked_at.dimmed(), status);
        }
    }

    let total_symbols = db.count_symbols()?;
    let locked_count = locks.len();
    println!(
        "\n{}/{} symbols locked",
        locked_count, total_symbols
    );

    Ok(())
}

fn cmd_symbols(repo: &str, file_filter: Option<&str>) -> Result<()> {
    let dir = grit_dir(repo);
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
        println!("  {} {} ({}){}", "|".dimmed(), name, kind.dimmed(), lock_indicator);
    }

    Ok(())
}

fn cmd_plan(repo: &str, agent: &str, intent: &str) -> Result<()> {
    let dir = grit_dir(repo);
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
        println!("  {} {}::{} [{}] {}", ">".dimmed(), file, name, kind, status);
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
            free.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(" ")
        );
    }

    Ok(())
}

fn cmd_done(repo: &str, agent: &str) -> Result<()> {
    let dir = grit_dir(repo);
    let lock_store = resolve_lock_store(repo)?;

    let locks = lock_store.locks_for_agent(agent)?;
    if locks.is_empty() {
        println!("Agent {} has no active locks.", agent);
        return Ok(());
    }


    println!("{} Agent {} finishing:", "+".green(), agent.bold());
    for (sym, _intent) in &locks {
        println!("  {} releasing {}", ">".dimmed(), sym);
    }

    // Try to merge worktree back
    let git_repo = GitRepo::open(repo)?;
    match git_repo.merge_worktree(agent) {
        Ok(()) => {
            println!("{} Merged branch agent/{}", "+".green(), agent);
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("does not exist") {
                // No worktree, that's fine
            } else {
                eprintln!("  warn: merge failed: {}", e);
            }
        }
    }

    // Clean up worktree
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

    let released = lock_store.release_all(agent)?;
    println!("{} Released {} symbols", "+".green(), released);

    // Notify
    let room = Room::new(&dir);
    room.notify(&RoomEvent {
        event_type: EventType::AgentDone,
        agent: agent.to_string(),
        symbols: locks.iter().map(|(s, _)| s.clone()).collect(),
    });

    Ok(())
}

fn cmd_watch(repo: &str) -> Result<()> {
    let dir = grit_dir(repo);
    let sock_path = dir.join("room.sock");

    if !sock_path.exists() {
        anyhow::bail!("No room socket found at {}. Run `grit init` first.", sock_path.display());
    }

    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixStream;

    println!("Connecting to room socket at {}...", sock_path.display());
    let stream = UnixStream::connect(&sock_path)?;
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
    let dir = grit_dir(repo);
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
    let dir = grit_dir(repo);
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
    let dir = grit_dir(repo);
    let db = Database::open(&dir.join("registry.db"))?;
    let git_repo = GitRepo::open(repo)?;

    let (name, branch, base) = db.get_active_session()?
        .ok_or_else(|| anyhow::anyhow!("No active session. Start one with: grit session start <name>"))?;

    // Check for remaining locks
    let lock_store = resolve_lock_store(repo)?;
    let locks = lock_store.all_locks()?;
    let worktrees = git_repo.list_worktrees()?;



    if !worktrees.is_empty() {
        println!("{} Warning: {} agents still have active worktrees:", "!".yellow(), worktrees.len());
        for wt in &worktrees {
            println!("  {} {}", ">".yellow(), wt);
        }
        println!("Run 'grit done -a <agent>' for each, or proceed anyway.\n");
    }

    if !locks.is_empty() {
        println!("{} Warning: {} symbols still locked", "!".yellow(), locks.len());
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
        name, branch, base, total_symbols, locks.len()
    );

    println!("Creating PR: {} -> {}", branch.cyan(), base.dimmed());
    let pr_url = git_repo.push_and_create_pr(&branch, pr_title, &body)?;

    println!("{} PR created: {}", "+".green(), pr_url);

    Ok(())
}

fn cmd_session_end(repo: &str, _name: Option<&str>) -> Result<()> {
    let dir = grit_dir(repo);
    let db = Database::open(&dir.join("registry.db"))?;
    let lock_store = resolve_lock_store(repo)?;
    let git_repo = GitRepo::open(repo)?;

    let (session_name, _branch, base) = db.get_active_session()?
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
    let dir = grit_dir(repo);
    let config = GritConfig {
        backend: "s3".to_string(),
        s3: Some(S3Config {
            bucket: bucket.to_string(),
            endpoint: endpoint.map(|s| s.to_string()),
            region: Some(region.to_string()),
            prefix: None,
        }),
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

fn cmd_config_set_local(repo: &str) -> Result<()> {
    let dir = grit_dir(repo);
    let config = GritConfig {
        backend: "local".to_string(),
        s3: None,
    };
    config.save(&dir)?;


    println!("{} Backend set to local (SQLite)", "+".green());

    Ok(())
}

fn cmd_config_show(repo: &str) -> Result<()> {
    let dir = grit_dir(repo);
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

    Ok(())
}
