FROM rust:1.99-slim

# Install necessary tools compilation dependencies
RUN apt-get update && apt-get install -y curl make git pkg-config libssl-dev && \
    rustup target add wasm32-unknown-unknown && \
    cargo install --locked stellar-cli@22.0.1

WORKDIR /app

# The local directory will be mapped here, so container just stays alive
CMD ["tail", "-f", "/dev/null"]
