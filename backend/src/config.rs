//! CLI and settings. Every setting is a command-line argument with a `DRA_*` environment fallback
//! (precedence: argument > environment > `.env` > default). There are no config files.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use axum::http::HeaderValue;
use clap::{Args, Parser, Subcommand, ValueEnum};
use secrecy::SecretString;
use uuid::Uuid;

use crate::shared::web::layers::HttpSettings;

#[derive(Debug, Parser)]
#[command(
    name = "dra-server",
    version,
    about = "dra-workflow backend: IT service disaster recovery planning"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[command(flatten)]
    pub log: LogArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the HTTP API.
    Serve(ServeArgs),
    /// Apply database migrations and exit.
    Migrate(DbArgs),
    /// Ensure the default DR roles exist in every tenant (or one tenant).
    SeedCatalog(SeedArgs),
    /// Export a plan version to a file for offline use (emergency handbook).
    Export(ExportArgs),
    /// Probe a running server's health endpoint; exits 0 when healthy.
    Healthcheck(HealthcheckArgs),
}

#[derive(Debug, Args)]
pub struct LogArgs {
    /// Log output format.
    #[arg(long, env = "DRA_LOG_FORMAT", value_enum, default_value_t = LogFormat::Pretty, global = true)]
    pub log_format: LogFormat,
    /// Log filter (tracing EnvFilter syntax). `RUST_LOG` is used when not set.
    #[arg(long, env = "DRA_LOG", global = true)]
    pub log: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Args)]
pub struct DbArgs {
    /// PostgreSQL connection URL (secret; never logged).
    #[arg(long, env = "DRA_DATABASE_URL", hide_env_values = true, value_parser = parse_secret)]
    pub database_url: SecretString,
    /// Maximum number of pooled database connections.
    #[arg(long, env = "DRA_DB_MAX_CONNECTIONS", default_value_t = 10)]
    pub db_max_connections: u32,
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    #[command(flatten)]
    pub db: DbArgs,

    /// Address to listen on.
    #[arg(long, env = "DRA_LISTEN_ADDR", default_value = "127.0.0.1:8090")]
    pub listen_addr: SocketAddr,

    /// Development mode: mocked authentication (admin `dev-user`), default tenant created on
    /// startup, migrations applied automatically. Never use in production.
    #[arg(long, env = "DRA_DEV_MODE")]
    pub dev_mode: bool,

    /// Slug of the default tenant in dev mode.
    #[arg(long, env = "DRA_DEV_TENANT", default_value = "demo")]
    pub dev_tenant: String,

    /// Apply pending migrations on startup (always on in dev mode).
    #[arg(long, env = "DRA_MIGRATE_ON_START")]
    pub migrate_on_start: bool,

    /// Allowed CORS origins (comma-separated). Empty: same-origin only.
    #[arg(long, env = "DRA_CORS_ORIGINS", value_delimiter = ',')]
    pub cors_origins: Vec<String>,

    /// Request timeout in seconds.
    #[arg(long, env = "DRA_REQUEST_TIMEOUT_SECS", default_value_t = 30)]
    pub request_timeout_secs: u64,

    /// Maximum request body size in bytes.
    #[arg(long, env = "DRA_BODY_LIMIT_BYTES", default_value_t = 1024 * 1024)]
    pub body_limit_bytes: usize,

    /// Maximum number of concurrently processed requests.
    #[arg(long, env = "DRA_MAX_CONCURRENCY", default_value_t = 512)]
    pub max_concurrency: usize,
}

impl ServeArgs {
    /// Validates and converts the HTTP settings (fail fast on invalid configuration).
    pub fn http_settings(&self) -> anyhow::Result<HttpSettings> {
        anyhow::ensure!(
            self.request_timeout_secs > 0,
            "--request-timeout-secs must be > 0"
        );
        anyhow::ensure!(
            self.body_limit_bytes >= 1024,
            "--body-limit-bytes must be >= 1024"
        );
        anyhow::ensure!(self.max_concurrency > 0, "--max-concurrency must be > 0");
        let cors_origins = self
            .cors_origins
            .iter()
            .map(|o| o.trim())
            .filter(|o| !o.is_empty())
            .map(|o| {
                anyhow::ensure!(
                    o != "*",
                    "wildcard CORS origin is not allowed; list origins explicitly"
                );
                HeaderValue::from_str(o).with_context(|| format!("invalid CORS origin `{o}`"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(HttpSettings {
            request_timeout: Duration::from_secs(self.request_timeout_secs),
            body_limit_bytes: self.body_limit_bytes,
            max_concurrency: self.max_concurrency,
            cors_origins,
        })
    }
}

#[derive(Debug, Args)]
pub struct SeedArgs {
    #[command(flatten)]
    pub db: DbArgs,
    /// Only seed this tenant (slug). Default: all tenants.
    #[arg(long)]
    pub tenant: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    Markdown,
    Json,
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    #[command(flatten)]
    pub db: DbArgs,
    /// Tenant slug.
    #[arg(long, env = "DRA_EXPORT_TENANT")]
    pub tenant: String,
    /// Plan version id to export.
    #[arg(long)]
    pub plan_version: Uuid,
    /// Output format.
    #[arg(long, value_enum, default_value_t = ExportFormat::Markdown)]
    pub format: ExportFormat,
    /// Output file (default: stdout).
    #[arg(long, short)]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct HealthcheckArgs {
    /// Address of the running server.
    #[arg(long, env = "DRA_LISTEN_ADDR", default_value = "127.0.0.1:8090")]
    pub addr: SocketAddr,
}

fn parse_secret(value: &str) -> Result<SecretString, String> {
    if value.trim().is_empty() {
        return Err("must not be empty".into());
    }
    Ok(SecretString::from(value.to_owned()))
}
