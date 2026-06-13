use toolpassport_backend::{
    DEFAULT_MAX_STORED_BYTES, StorageService, app_with_storage, connect_and_migrate,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "toolpassport_backend=info".into()),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://../data/toolpassport.db".into());
    let pool = connect_and_migrate(&database_url)
        .await
        .expect("backend database must connect and migrate");
    let artifact_root = std::env::var("ARTIFACT_ROOT").unwrap_or_else(|_| "../runs".into());
    let artifact_max_bytes = std::env::var("ARTIFACT_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_MAX_STORED_BYTES);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .expect("backend listener must bind");

    axum::serve(
        listener,
        app_with_storage(pool, StorageService::new(artifact_root, artifact_max_bytes)),
    )
    .await
    .expect("backend server must run");
}
