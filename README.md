# websocker-server

## Description

This project is a WebSocket server implemented in Rust. It allows clients to connect and communicate in real-time.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Docker](https://docs.docker.com/get-docker/)

## Installation

1. Clone the repository:
    ```sh
    git clone https://github.com/raushanraja/websocket-server.git
    cd websocket-server
    ```

2. Build the project:
    ```sh
    cargo build --release
    ```

## Usage

1. Run the WebSocket server:
    ```sh
    cargo run --release
    ```

2. Connect to the WebSocket server using a WebSocket client.

## Docker

1. Build the Docker image:
    ```sh
    docker build -t websocket-server .
    ```

2. Run the Docker container:
    ```sh
    docker run -p 8080:8080 websocket-server 
    ```

## Contributing

1. Fork the repository.
2. Create a new branch (`git checkout -b feature-branch`).
3. Make your changes.
4. Commit your changes (`git commit -am 'Add new feature'`).
5. Push to the branch (`git push origin feature-branch`).
6. Create a new Pull Request.

