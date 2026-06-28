use backend::{db::open_database, routes::router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "backend=info,tower_http=info".into()),
        )
        .init();

    let path =
        std::env::var("STUDY_SCHEDULER_DB").unwrap_or_else(|_| "study-scheduler.db".to_string());
    let conn = open_database(path)?;
    let app = router(conn);
    let listener = TcpListener::bind("127.0.0.1:5174").await?;
    tracing::info!("Study Scheduler API listening on http://127.0.0.1:5174");
    axum::serve(listener, app).await?;
    Ok(())
}
