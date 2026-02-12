#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::{routing::get, Router};
    use leptos::config::get_configuration;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use attractor_web::App;
    use attractor_web::server::documents::DocumentWatcher;
    use std::path::PathBuf;
    use std::sync::Arc;

    tracing_subscriber::fmt::init();

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    // Initialize file watcher for .attractor/ directory
    let attractor_dir = PathBuf::from(".attractor");
    let doc_watcher = Arc::new(
        DocumentWatcher::new(attractor_dir.clone())
            .expect("Failed to start document file watcher"),
    );

    let app_state = attractor_web::server::AppState {
        doc_watcher: doc_watcher.clone(),
        attractor_dir,
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
            move || {},
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    tracing::info!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
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
                <script type="module">
                    r#"
                    import { Terminal } from 'https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/+esm';
                    import { FitAddon } from 'https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/+esm';
                    import { WebLinksAddon } from 'https://cdn.jsdelivr.net/npm/@xterm/addon-web-links@0.11.0/+esm';

                    window.initTerminal = function(containerId) {
                        const container = document.getElementById(containerId);
                        if (!container) return;

                        const terminal = new Terminal({
                            cursorBlink: true,
                            fontSize: 14,
                            fontFamily: '"SF Mono", Monaco, "Cascadia Code", "Roboto Mono", Consolas, monospace',
                            theme: {
                                background: '#1e1e2e',
                                foreground: '#cdd6f4',
                                cursor: '#f5e0dc',
                                selectionBackground: '#585b70',
                                black: '#45475a',
                                red: '#f38ba8',
                                green: '#a6e3a1',
                                yellow: '#f9e2af',
                                blue: '#89b4fa',
                                magenta: '#f5c2e7',
                                cyan: '#94e2d5',
                                white: '#bac2de',
                                brightBlack: '#585b70',
                                brightRed: '#f38ba8',
                                brightGreen: '#a6e3a1',
                                brightYellow: '#f9e2af',
                                brightBlue: '#89b4fa',
                                brightMagenta: '#f5c2e7',
                                brightCyan: '#94e2d5',
                                brightWhite: '#a6adc8',
                            },
                            allowProposedApi: true,
                        });

                        const fitAddon = new FitAddon();
                        terminal.loadAddon(fitAddon);
                        terminal.loadAddon(new WebLinksAddon());
                        terminal.open(container);
                        fitAddon.fit();

                        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
                        const wsUrl = protocol + '//' + window.location.host + '/api/terminal/ws';
                        const ws = new WebSocket(wsUrl);
                        ws.binaryType = 'arraybuffer';

                        ws.onopen = function() {
                            ws.send(JSON.stringify({ type: 'resize', cols: terminal.cols, rows: terminal.rows }));
                        };
                        ws.onmessage = function(event) {
                            if (event.data instanceof ArrayBuffer) {
                                terminal.write(new Uint8Array(event.data));
                            } else {
                                terminal.write(event.data);
                            }
                        };
                        ws.onclose = function() {
                            terminal.write('\r\n\x1b[31m[Connection closed]\x1b[0m\r\n');
                        };
                        terminal.onData(function(data) {
                            if (ws && ws.readyState === WebSocket.OPEN) {
                                ws.send(new TextEncoder().encode(data));
                            }
                        });
                        terminal.onResize(function(size) {
                            if (ws && ws.readyState === WebSocket.OPEN) {
                                ws.send(JSON.stringify({ type: 'resize', cols: size.cols, rows: size.rows }));
                            }
                        });
                        new ResizeObserver(function() { fitAddon.fit(); }).observe(container);
                    };
                    "#
                </script>
            </body>
        </html>
    }
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
