FROM rust:1.79-buster

WORKDIR /usr/src/app
COPY . .
RUN cargo install --path .
+EXPOSE 8181
ENTRYPOINT ["wsserver"]
