# Étape 1 : Construction (Builder)
FROM rust:bookworm AS builder

WORKDIR /usr/src/app
# On copie les manifestes et on crée un dummy pour mettre en cache les dépendances
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo "fn main() {println!(\"if you see this, the build broke\")}" > src/main.rs \
    && cargo build --release \
    && rm -rf src

# Copie du code source complet et compilation réelle
COPY src ./src
COPY keycloak ./keycloak
# La commande 'touch' permet de forcer la recompilation du main.rs
RUN touch src/main.rs && cargo build --release

# Étape 2 : Runtime léger
FROM debian:bookworm-slim

# Installation des certificats racine SSL pour Keycloak et reqwest
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /usr/src/app/target/release/truegather-backend /app/truegather-backend
# Parfois on a besoin de certains éléments de config présents dans le dossier backend
# COPY --from=builder /usr/src/app/keycloak /app/keycloak

ENV RUST_LOG=info

# Le port d'écoute d'axum
EXPOSE 8080

CMD ["./truegather-backend"]
