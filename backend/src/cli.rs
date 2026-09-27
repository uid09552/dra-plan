//! Subcommand implementations (the binary's edge: `anyhow` is fine here).

use std::time::Duration;

use anyhow::Context;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing_subscriber::EnvFilter;

use crate::bootstrap::{self, AuthMode};
use crate::config::{
    Cli, Command, DbArgs, ExportArgs, ExportFormat, HealthcheckArgs, LogArgs, LogFormat, SeedArgs,
    ServeArgs,
};
use crate::features::directory::domain::DEFAULT_ROLES;
use crate::features::plans::api::PlanVersionDto;
use crate::features::plans::application::Export;
use crate::shared::infra::Db;
use crate::shared::kernel::{Principal, TenantContext, TenantRole};

pub fn init_tracing(log: &LogArgs) {
    let filter = log.log.as_deref().map(EnvFilter::new).unwrap_or_else(|| {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    });
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match log.log_format {
        LogFormat::Pretty => builder.init(),
        LogFormat::Json => builder.json().flatten_event(true).init(),
    }
}

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Serve(args) => serve(args).await,
        Command::Migrate(args) => migrate(args).await,
        Command::SeedCatalog(args) => seed_catalog(args).await,
        Command::Export(args) => export(args).await,
        Command::Healthcheck(args) => healthcheck(args).await,
    }
}

async fn connect(db: &DbArgs) -> anyhow::Result<Db> {
    Db::connect(&db.database_url, db.db_max_connections)
        .await
        .context("cannot connect to the database")
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let http = args.http_settings()?;
    let db = connect(&args.db).await?;
    if args.dev_mode || args.migrate_on_start {
        db.migrate().await.context("migration failed")?;
        tracing::info!("database migrations applied");
    }
    let auth = if args.dev_mode {
        AuthMode::Dev {
            tenant_slug: args.dev_tenant.clone(),
        }
    } else {
        AuthMode::Disabled
    };
    let router = bootstrap::build_router(db, auth, &http).await?;

    let listener = tokio::net::TcpListener::bind(args.listen_addr)
        .await
        .with_context(|| format!("cannot listen on {}", args.listen_addr))?;
    tracing::info!(addr = %args.listen_addr, dev_mode = args.dev_mode, "dra-server listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("shut down");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

async fn migrate(args: DbArgs) -> anyhow::Result<()> {
    connect(&args)
        .await?
        .migrate()
        .await
        .context("migration failed")?;
    tracing::info!("database migrations applied");
    Ok(())
}

/// Ensures every tenant (or the given one) has the default DR roles.
async fn seed_catalog(args: SeedArgs) -> anyhow::Result<()> {
    let db = connect(&args.db).await?;
    db.migrate().await.context("migration failed")?;
    let added = db
        .seed_default_roles(args.tenant.as_deref(), DEFAULT_ROLES)
        .await?;
    println!("added {added} default role(s)");
    Ok(())
}

/// Writes a plan version to a file (or stdout) for offline use during a disaster.
async fn export(args: ExportArgs) -> anyhow::Result<()> {
    let db = connect(&args.db).await?;
    let state = bootstrap::build_state(db);
    let tenant = state
        .tenants
        .find_by_slug(&args.tenant)
        .await?
        .with_context(|| format!("unknown tenant `{}`", args.tenant))?;
    let ctx = TenantContext {
        tenant_id: tenant.id(),
        principal: Principal {
            user: "cli-export".into(),
            role: TenantRole::Auditor,
        },
    };
    let lang = tenant.settings.default_language;
    let content = match state
        .plans
        .export(
            &ctx,
            args.plan_version,
            matches!(args.format, ExportFormat::Markdown),
            lang,
        )
        .await?
    {
        Export::Markdown(text) => text,
        Export::Json(version) => serde_json::to_string_pretty(&PlanVersionDto::new(version)?)?,
    };
    match args.output {
        Some(path) => {
            std::fs::write(&path, content)
                .with_context(|| format!("cannot write {}", path.display()))?;
            eprintln!("exported to {}", path.display());
        }
        None => println!("{content}"),
    }
    Ok(())
}

/// Minimal HTTP/1.1 probe (no HTTP client dependency needed).
async fn healthcheck(args: HealthcheckArgs) -> anyhow::Result<()> {
    let probe = async {
        let mut stream = TcpStream::connect(args.addr).await?;
        let request = format!(
            "GET /api/v1/health HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            args.addr
        );
        stream.write_all(request.as_bytes()).await?;
        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        anyhow::Ok(response)
    };
    let response = tokio::time::timeout(Duration::from_secs(5), probe)
        .await
        .context("health check timed out")??;
    let status_line = response.lines().next().unwrap_or_default();
    anyhow::ensure!(status_line.contains(" 200 "), "unhealthy: {status_line}");
    println!("healthy");
    Ok(())
}
