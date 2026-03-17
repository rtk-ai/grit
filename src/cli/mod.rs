use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::db::Database;
use crate::git::GitRepo;
use crate::parser::SymbolIndex;
use crate::room::{LockResult, Room, RoomEvent, EventType, NotificationServer};

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

pub fn run(cli: Cli) -> Result<()> {
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
        Command::Heartbeat { agent, ttl } => cmd_heartbeat(&cli.repo, &agent, ttl),
    }
}

fn grit_dir(repo: &str) -> std::path::PathBuf {
    std::path::Path::new(repo).join(".grit")
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
    let db = Database::open(&dir.join("registry.db"))?;

    let mut granted = Vec::new();
    let mut blocked = Vec::new();

    for sym_id in symbols {
        match db.try_lock(sym_id, agent, intent, ttl)? {
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
                use colored::Colorize;
                println!("{} Worktree: {}", "+".cyan(), wt_path.display());
            }
            Err(e) => {
                // Worktree may already exist, that's fine
                let msg = format!("{}", e);
                if !msg.contains("already exists") {
                    eprintln!("  warn: could not create worktree: {}", e);
                }
            }
        }
    }

    if !granted.is_empty() {
        use colored::Colorize;
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
        use colored::Colorize;
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
    let db = Database::open(&dir.join("registry.db"))?;

    if symbols.is_empty() {
        let released = db.release_all(agent)?;
        println!("Released {} symbols for {}", released, agent);
    } else {
        for sym_id in symbols {
            db.release(sym_id, agent)?;
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
    let db = Database::open(&dir.join("registry.db"))?;

    let locks = db.all_locks()?;

    if locks.is_empty() {
        println!("No active locks.");
        return Ok(());
    }

    use colored::Colorize;

    // Group by agent
    let mut by_agent: std::collections::BTreeMap<String, Vec<(String, String, String, u64)>> =
        std::collections::BTreeMap::new();
    for (sym, agent, intent, ts, ttl) in &locks {
        by_agent
            .entry(agent.clone())
            .or_default()
            .push((sym.clone(), intent.clone(), ts.clone(), *ttl));
    }

    for (agent, syms) in &by_agent {
        let intent = &syms[0].1;
        println!("{} {} -- {}", "*".green(), agent.bold(), intent.dimmed());
        for (sym, _, ts, ttl) in syms {
            let expired = db.is_lock_expired(sym).unwrap_or(false);
            let status = if expired {
                "EXPIRED".red().to_string()
            } else {
                format!("ttl={}s", ttl)
            };
            println!("  {} {} ({}) [{}]", "|".dimmed(), sym, ts.dimmed(), status);
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

    use colored::Colorize;

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

    use colored::Colorize;
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
    let db = Database::open(&dir.join("registry.db"))?;

    let locks = db.locks_for_agent(agent)?;
    if locks.is_empty() {
        println!("Agent {} has no active locks.", agent);
        return Ok(());
    }

    use colored::Colorize;
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
            let msg = format!("{}", e);
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
            let msg = format!("{}", e);
            if !msg.contains("not found") && !msg.contains("does not exist") {
                eprintln!("  warn: could not remove worktree: {}", e);
            }
        }
    }

    let released = db.release_all(agent)?;
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
                        use colored::Colorize;
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

    use colored::Colorize;
    println!("{}", "Active worktrees:".bold());
    for agent_id in &worktrees {
        let dir = grit_dir(repo).join("worktrees").join(agent_id);
        println!("  {} {} -> {}", ">".green(), agent_id, dir.display());
    }

    Ok(())
}

fn cmd_gc(repo: &str) -> Result<()> {
    let dir = grit_dir(repo);
    let db = Database::open(&dir.join("registry.db"))?;

    let expired = db.gc_expired_locks()?;
    if expired == 0 {
        println!("No expired locks found.");
    } else {
        println!("Cleaned up {} expired locks.", expired);
    }

    Ok(())
}

fn cmd_heartbeat(repo: &str, agent: &str, ttl: u64) -> Result<()> {
    let dir = grit_dir(repo);
    let db = Database::open(&dir.join("registry.db"))?;

    let refreshed = db.refresh_ttl(agent, ttl)?;
    if refreshed == 0 {
        println!("Agent {} has no active locks to refresh.", agent);
    } else {
        println!("Refreshed TTL for {} locks (new ttl={}s).", refreshed, ttl);
    }

    Ok(())
}
