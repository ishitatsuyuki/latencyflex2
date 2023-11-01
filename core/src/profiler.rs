use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::thread;

use axum::extract::ws::WebSocket;
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum::{Router, Server};
use tokio::runtime;
use tokio::sync::broadcast;

struct ProfilerServer {
    state: Arc<Mutex<AppState>>,
}

struct AppState {
    sender: broadcast::Sender<()>,
}

async fn handler(State(state): State<Arc<Mutex<AppState>>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|socket| handle_socket(state, socket))
}

async fn handle_socket(state: Arc<Mutex<AppState>>, mut socket: WebSocket) {
    let mut sub = state.lock().unwrap().sender.subscribe();
    while let Ok(msg) = sub.recv().await {
        if socket.send(todo!()).await.is_err() {
            // client disconnected
            return;
        }
    }
}

impl ProfilerServer {
    const DEFAULT_PORT: u16 = 5392;

    fn new() {
        let state = Arc::new(Mutex::new(AppState {
            sender: broadcast::Sender::new(64),
        }));
        let app = Router::new().route("/ws", get(handler)).with_state(state);

        for port in Self::DEFAULT_PORT..Self::DEFAULT_PORT + 16 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
            if let Ok(server) = Server::try_bind(&addr) {
                println!("Listening on {}", addr);

                // TODO: handle shutdown
                thread::spawn(move || {
                    let runtime = runtime::Builder::new_current_thread().build().unwrap();
                    runtime.block_on(server.serve(app.into_make_service()))
                });

                break;
            }
        }
    }
}
