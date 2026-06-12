use toolpassport_backend::{app, connect_and_migrate};

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

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .expect("backend listener must bind");

    axum::serve(listener, app(pool))
        .await
        .expect("backend server must run");
}
