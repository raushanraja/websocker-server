FROM rust:1.79-buster

WORKDIR /usr/src/app
COPY . .
RUN cargo install --path .
ENTRYPOINT ["wsserver"]
