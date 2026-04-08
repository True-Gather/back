FROM rust:1.88

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations

RUN cargo build

EXPOSE 8080

CMD ["cargo", "run"]