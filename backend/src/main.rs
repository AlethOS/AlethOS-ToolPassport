use toolpassport_backend::app;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "toolpassport_backend=info".into()),
        )
        .init();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .expect("backend listener must bind");

    axum::serve(listener, app())
        .await
        .expect("backend server must run");
}
