#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::{routing::get, Router};
    use leptos::config::get_configuration;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use attractor_web::App;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;

    tracing_subscriber::fmt::init();

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    // Initialize SQLite database
    let db = attractor_web::server::db::init_db()
        .await
        .expect("Failed to initialize database");

    let terminal_sessions = attractor_web::server::terminal::TerminalSessions::default();

    let app_state = attractor_web::server::AppState {
        db: db.clone(),
        watchers: Arc::new(Mutex::new(HashMap::new())),
        terminal_sessions,
    };

    // API routes with AppState
    let api_routes = Router::new()
        .route(
            "/api/terminal/ws",
            get(attractor_web::server::terminal::ws_terminal),
        )
        .route(
            "/api/documents/stream",
            get(attractor_web::server::documents::document_stream),
        )
        .route(
            "/api/stream/{session_id}",
            get(attractor_web::server::stream::stream_events),
        )
        .with_state(app_state);

    // Leptos routes with LeptosOptions state
    let app = api_routes
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let pool = db.clone();
                move || {
                    leptos::prelude::provide_context(pool.clone());
                }
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    tracing::info!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap_or_else(|e| {
        panic!("Failed to bind to {addr}: {e}. Is another instance already running?");
    });
    let app = app.into_make_service();
    axum::serve(listener, app).await.unwrap();
}

#[cfg(feature = "ssr")]
fn shell(options: leptos::config::LeptosOptions) -> impl leptos::IntoView {
    use leptos::prelude::*;
    use leptos_meta::*;
    use attractor_web::App;

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
                <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.min.css"/>
            </head>
            <body>
                <App/>
                <script type="module" src="/js/xterm-setup.js"></script>
            </body>
        </html>
    }
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
