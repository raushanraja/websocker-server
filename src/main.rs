use futures_util::*;
use rand;
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
                if let Some(client) = clients.get(&id) {
                    println!(
                        "Sending new message client_id: {}, message: {}",
                        client.id, message
                    );
                    if let Err(e) = client.sender.send(message.clone()) {
                        eprintln!("Failed to send message: {}", e);
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
async fn handle_connection(
    tx: mpsc::UnboundedSender<ServerMessage>,
    clients: Arc<RwLock<HashMap<u64, CClient>>>,
    ws_stream: WebSocketStream<TcpStream>,
) {
    let _count = 0;
    let (mut outgoing, incoming) = ws_stream.split();
    let client_id = next_id();
    let (message_tx, mut message_rx) = mpsc::unbounded_channel();

    let client = CClient {
        id: client_id,
        sender: message_tx.clone(),
    };

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

        msg = incoming.next() => {
            match msg {
                Some(Ok(msg)) => {
                    // handle_message(server, msg)
                    println!("Received message: {}", msg);

                    for (id, client) in clients.read().await.iter() {
                        if *id == client_id {continue}
                        if let Err(e) = client.sender.send(msg.to_string()) {
                            eprintln!("Failed to send message: {}", e);
                        }
                    }
               }
                _ => {
                    if let Err(e) = txc.send(ServerMessage::RemoveClient(client_id)) {
                        eprintln!("Failed to send remove client message: {}", e);
                    }
                    break;
                }
            }
        }
                    _ = ping_interval.tick() => {
                           // txc.send(ServerMessage::Message(client_id, String::from("Ping"))).unwrap();
                    }
                }
    }
}

fn process_request(req: &Request, res: Response) -> Result<Response, ErrorResponse> {
    println!("Processing request: {:?}", req);

    let headers = req.headers();
    let protocol = headers
        .get("Sec-WebSocket-Protocol")
        .map(|v| v.to_str().unwrap())
        .unwrap_or("unknown");

    let username = headers
        .get("Sec-WebSocket-Username")
        .map(|v| v.to_str().unwrap())
        .unwrap_or("unknown");

    let password = headers
        .get("Sec-WebSocket-Password")
        .map(|v| v.to_str().unwrap())
        .unwrap_or("unknown");

    println!(
        "Protocol: {}, Username: {}, Password: {}",
        protocol, username, password
    );

    Ok(res)
}

#[tokio::main]
async fn main() {
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
            let tx = server.tx.clone();
            let clients = server.clients.clone();
            match tokio_tungstenite::accept_hdr_async(stream, process_request).await {
                Ok(connection) => {
                    let join_handle = handle_connection(tx, clients, connection);
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
