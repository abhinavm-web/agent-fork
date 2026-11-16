use thiserror::Error;

#[derive(Error, Debug)]
pub enum AgentForkError {
    #[error("no active agent-fork session found in '{0}'; run 'agent-fork checkpoint' or 'agent-fork run' to initialize")]
    SessionNotFound(String),

    #[error("step '{0}' not found in session timeline")]
    StepNotFound(String),

    #[error("branch '{0}' not found")]
    BranchNotFound(String),

    #[error("branch '{0}' already exists")]
    BranchAlreadyExists(String),

    #[error("working tree contains uncheckpointed modifications; use '--force' to overwrite or 'agent-fork checkpoint' to save changes")]
    DirtyWorkingTree,

    #[error("cassette '{0}' not found")]
    CassetteNotFound(String),

    #[error("failed to execute child process '{command}': {source}")]
    ProcessSpawnFailed {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("IO error at '{path}': {source}")]
    IoWithPath {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("proxy error: {0}")]
    Proxy(String),

    #[error("cassette error: {0}")]
    Cassette(String),

    #[error("CAS storage error: {0}")]
    Cas(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("ignore walker error: {0}")]
    Ignore(#[from] ignore::Error),
}
