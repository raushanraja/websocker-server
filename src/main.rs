use futures_util::*;
use r2d2_redis::redis::Commands;
use rand;
use rpool::R2D2Pool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::join;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tokio_tungstenite::WebSocketStream;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::protocol::Message;

mod rpool;

// Define a struct to hold client's connection state
#[derive(Debug, Clone)]
struct CClient {
    id: u64,
    sender: mpsc::UnboundedSender<String>,
}

// Define a enum for server messages sebt bwtween the
// server and client threads
#[derive(Debug, Clone)]
enum ServerMessage {
    NewClient(CClient),
    Message(u64, String),
    RemoveClient(u64),
}

// Define a struct to hold the server state
struct SServer {
    clients: Arc<RwLock<HashMap<u64, CClient>>>,
    tx: mpsc::UnboundedSender<ServerMessage>,
}

// MEthod to get a randomId
fn next_id() -> u64 {
    rand::random()
}

// Define a method to handle incoming messages
fn handle_message(server: Arc<SServer>, msg: ServerMessage) {
    match msg {
        ServerMessage::NewClient(client) => {
            println!("New client connected: {}", client.id);
            match server.clients.try_write() {
                Ok(mut clients) => {
                    clients.insert(client.id, client);
                }
                Err(e) => {
                    eprintln!("Failed to acquire write lock: {}", e);
                }
            }
        }
        ServerMessage::Message(id, message) => match server.clients.try_read() {
            Ok(clients) => {
                for (client_id, client) in clients.iter() {
                    if *client_id != id {
                        println!(
                            "Sending new message to client_id: {}, message: {}",
                            client.id, message
                        );
                        if let Err(e) = client.sender.send(message.clone()) {
                            eprintln!("Failed to send message to client {}: {}", client.id, e);

                            // Match error with "channel closed" Error
                            if e.to_string().contains("channel closed") {
                                let tx = server.tx.clone();
                                if let Err(e) = tx.send(ServerMessage::RemoveClient(client.id)) {
                                    eprintln!("Failed to send remove client message: {}", e);
                                } else {
                                    println!(
                                        "Client {} disconnected, removing from clients list",
                                        client.id
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to acquire read lock: {}", e);
            }
        },
        ServerMessage::RemoveClient(id) => match server.clients.try_write() {
            Ok(mut clients) => {
                clients.remove(&id);
                println!("client {} disconnected", id);
            }
            Err(e) => {
                eprintln!("Failed to acquire write lock: {}", e);
            }
        },
    }
}
// Define a method to handle incoming WebSocket connections
async fn handle_connection(server: Arc<SServer>, ws_stream: WebSocketStream<TcpStream>) {
    let _count = 0;
    let (mut outgoing, incoming) = ws_stream.split();
    let client_id = next_id();
    let (message_tx, mut message_rx) = mpsc::unbounded_channel();

    let client = CClient {
        id: client_id,
        sender: message_tx.clone(),
    };

    let tx = server.tx.clone();

    if let Err(e) = tx.send(ServerMessage::NewClient(client.clone())) {
        eprintln!("Failed to send new client message: {}", e);
    }
    let txc: mpsc::UnboundedSender<ServerMessage> = tx.clone();

    let mut ping_interval = interval(Duration::from_secs(1));
    let mut incoming = incoming.map_err(|e| {
        println!("Error {}", e);
        // txc.send(ServerMessage::RemoveClient(client_id)).unwrap();
    });

    let _msc = message_tx.clone();
    loop {
        tokio::select! {
                        message = message_rx.recv() =>{
                            match  message {
                                    Some(msg) => {
                                        let _ = outgoing.send(Message::Text(msg)).await;
                                    },
                                    None => println!("None"),
                                }
                        },

                        message = incoming.next() => {
                            match message {
                                Some(Ok(msg)) => {
                                    let message = msg.into_text();
                                    match message {
                                        Ok(message) => {
                                            handle_message(server.clone(), ServerMessage::Message(client_id, message));
                                        },
                                        Err(e) => {
                                            eprintln!("Failed to convert message to text: {}", e);
                                        }
                                    }
                                },
                                Some(Err(e)) => {
                                    eprintln!("Failed to receive message: {:?}", e);
                                    if let Err(e) = txc.send(ServerMessage::RemoveClient(client_id)) {
                                        eprintln!("Failed to send remove client message: {}", e);
                                    }
                                    break;
                                },
                                None => {
                                    eprintln!("Connection closed");
                                    if let Err(e) = txc.send(ServerMessage::RemoveClient(client_id)) {
                                        eprintln!("Failed to send remove client message: {}", e);
                                    }
                                    break;
                                }
                            }

                        },
        }
    }
}

fn process_request(
    req: &Request,
    res: Response,
    r2d2_pool: R2D2Pool,
) -> Result<Response, ErrorResponse> {
    println!("Processing request: {:?}", req);

    let headers = req.headers();

    let username = headers
        .get("Sec-WebSocket-Username")
        .map(|v| v.to_str().unwrap())
        .unwrap_or("unknown");

    let password = headers
        .get("Sec-WebSocket-Password")
        .map(|v| v.to_str().unwrap())
        .unwrap_or("unknown");

    if (username == "unknown" || password == "unknown") {
        return Err(ErrorResponse::new(Some("401 Unauthorized".to_string())));
    }

    // Get a connection from the pool
    let mut conn = r2d2_pool.get().expect("Failed to get connection from pool");

    // Get the password from redis
    match conn.get::<&str, String>(username) {
        Ok(p) => {
            if p == password {
                return Ok(res);
            }
            Err(ErrorResponse::new(Some("401 Unauthorized".to_string())))
        }
        Err(e) => {
            eprintln!("Failed to get password from redis: {}", e);
            return Err(ErrorResponse::new(Some("401 Unauthorized".to_string())));
        }
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().expect(".env file not found");

    let r2d2_pool = rpool::connect().expect(
        "Failed to create connection pool to redis. Please check your redis connection string",
    );

    for (key, value) in std::env::vars() {
        if (key.contains("username_")) {
            let username = key.replace("username_", "");
            let password = value;

            let mut conn = r2d2_pool.get().expect("Failed to get connection from pool");
            let _: () = conn
                .set(username, password)
                .expect("Failed to set username and password in redis");
        }
    }

    // Add username and password to redis

    let (tx, mut rx) = mpsc::unbounded_channel();
    let server = Arc::new(SServer {
        clients: Arc::new(RwLock::new(HashMap::new())),
        tx,
    });

    let message_handler_server = server.clone();
    let server_message_handler = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            handle_message(message_handler_server.clone(), message);
        }
    });

    let server_connection_handler = tokio::spawn(async move {
        let listener = match TcpListener::bind("0.0.0.0:8181").await {
            Ok(listener) => listener,
            Err(e) => {
                eprintln!("Failed to bind to address: {}", e);
                return;
            }
        };

        while let Ok((stream, _)) = listener.accept().await {
            match tokio_tungstenite::accept_hdr_async(stream, |req: &Request, res: Response| {
                process_request(req, res, r2d2_pool.clone())
            })
            .await
            {
                Ok(connection) => {
                    let join_handle = handle_connection(server.clone(), connection);
                    tokio::spawn(join_handle);
                }
                Err(e) => {
                    eprintln!("Failed to accept connection: {}", e);
                }
            }
        }
    });

    join!(server_connection_handler, server_message_handler);
}
