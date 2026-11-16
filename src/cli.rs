use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "agent-fork",
    about = "OS-level Copy-on-Write time-travel debugger and parallel trajectory forker for AI coding agents",
    long_about = "agent-fork wraps coding agents (Claude Code, Codex, custom LLM CLI scripts) in a Copy-on-Write sandbox paired with an LLM/MCP cassette proxy.\nIt intercepts filesystem mutations and external API turns, allowing instant checkpointing, historical step rewinding, and parallel trajectory forking.",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Override the workspace root directory (defaults to current working directory)
    #[arg(short = 'w', long = "workspace", global = true)]
    pub workspace: Option<PathBuf>,

    /// Run with verbose tracing output
    #[arg(long = "verbose", global = true)]
    pub verbose: bool,

    /// Suppress all non-essential output
    #[arg(short = 'q', long = "quiet", global = true)]
    pub quiet: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Execute an agent command wrapped in the CoW sandbox and cassette proxy
    #[command(trailing_var_arg = true)]
    Run {
        /// Optional description or message for this execution turn
        #[arg(short = 'm', long = "message")]
        message: Option<String>,

        /// Disable the LLM/MCP cassette proxy (record filesystem mutations only)
        #[arg(long = "no-proxy")]
        no_proxy: bool,

        /// Explicit port to bind the cassette proxy to (default: ephemeral random port)
        #[arg(long = "proxy-port", default_value_t = 0)]
        proxy_port: u16,

        /// Upstream Anthropic API base URL
        #[arg(long = "upstream-anthropic", default_value = "https://api.anthropic.com")]
        upstream_anthropic: String,

        /// Upstream OpenAI API base URL
        #[arg(long = "upstream-openai", default_value = "https://api.openai.com")]
        upstream_openai: String,

        /// The agent command and arguments to execute
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
    },

    /// Create an explicit manual snapshot checkpoint of the workspace
    Checkpoint {
        /// Human-readable description of this checkpoint
        #[arg(short = 'm', long = "message", default_value = "Manual checkpoint")]
        message: String,
    },

    /// Display the session timeline, step checkpoints, and modification stats
    Log {
        /// Maximum number of historical steps to display
        #[arg(short = 'n', long = "limit", default_value_t = 20)]
        limit: usize,

        /// Output timeline in structured JSON format
        #[arg(long = "json")]
        json: bool,
    },

    /// Show unified diff of filesystem changes between steps or against the working tree
    Diff {
        /// Source step ID to compare from (defaults to immediate predecessor of target)
        step_a: Option<String>,

        /// Target step ID to compare to (defaults to current working directory)
        step_b: Option<String>,

        /// Show only insertion and deletion line statistics
        #[arg(long = "stat")]
        stat: bool,

        /// Show only changed file names
        #[arg(long = "name-only")]
        name_only: bool,
    },

    /// Rewind workspace files and session pointer to a historical step
    Rewind {
        /// Historical step ID or index to rewind to (e.g., "14" or "step_0014")
        step: String,

        /// Overwrite uncheckpointed dirty changes in the workspace
        #[arg(short = 'f', long = "force")]
        force: bool,

        /// Clean up untracked files added after the target step
        #[arg(long = "clean", default_value_t = true)]
        clean: bool,
    },

    /// Fork parallel isolated branch worktrees from a historical step
    Fork {
        /// Base step ID to fork from
        step: String,

        /// Branch variant names to create (can be repeated for multiple variants)
        #[arg(short = 'v', long = "variant", required = true, num_args = 1..)]
        variants: Vec<String>,
    },

    /// List active branches and their current step pointers
    Branches {
        /// Output branch list in structured JSON format
        #[arg(long = "json")]
        json: bool,
    },

    /// Promote a variant branch's changes back into the primary workspace
    Promote {
        /// Name of the variant branch to promote
        branch: String,

        /// Overwrite uncheckpointed dirty changes in the base workspace
        #[arg(short = 'f', long = "force")]
        force: bool,
    },

    /// Inspect, show, or edit recorded LLM/MCP cassette turns
    Cassette {
        #[command(subcommand)]
        action: CassetteAction,
    },

    /// Install, uninstall, or inspect zero-touch Claude Code and Git checkpoint hooks
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum HookAction {
    /// Install per-tool-call Claude Code PostToolUse hook and Git post-commit hook
    Install,
    /// Remove installed agent-fork hooks from the workspace
    Uninstall,
    /// Check installation status of agent-fork hooks
    Status,
}

#[derive(Subcommand, Debug)]
pub enum CassetteAction {
    /// List recorded cassettes for a step or the entire session
    List {
        /// Filter by specific step ID
        step: Option<String>,
    },

    /// Display request and response payload for a specific cassette turn
    Show {
        /// Cassette ID to display
        id: String,
    },

    /// Edit prompt or tool response in a recorded cassette
    Edit {
        /// Cassette ID to edit
        id: String,

        /// Overwrite request prompt body
        #[arg(long = "prompt")]
        prompt: Option<String>,

        /// Overwrite response content body
        #[arg(long = "response")]
        response: Option<String>,
    },

    /// Remove all recorded cassettes for the session
    Clear,
}
