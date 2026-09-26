# Windows/WSL2 Setup Guide

Astera's contracts and tooling are built for a Unix-like environment. On Windows
we **strongly** recommend the Windows Subsystem for Linux 2 (WSL2) rather than
native Windows, PowerShell, or Git Bash. This guide gets you from a fresh Windows
install to a working local Astera development environment.

## 1. Install WSL2

Open **PowerShell as Administrator** and run:

```powershell
wsl --install
```

This installs Ubuntu (by default) and enables the WSL2 virtual machine
platform. Reboot when prompted, then launch **Ubuntu** from the Start menu and
create your Linux user account.

> If you already have WSL1, upgrade it: `wsl --set-default-version 2` and, for an
> existing distro, `wsl --set-version Ubuntu 2`.

## 2. Install dependencies inside WSL2 (Ubuntu)

Open your Ubuntu terminal and install the toolchain:

```bash
sudo apt update && sudo apt upgrade -y
sudo apt install -y build-essential pkg-config libssl-dev curl git python3
```

### Rust + the Soroban wasm target

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown
```

### Stellar CLI

```bash
cargo install --locked stellar-cli
```

Verify with `stellar --version`.

### Node.js 20+

Use `nvm` (recommended) or your distro's packages:

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
nvm install 20
```

### Freighter wallet

Install the [Freighter](https://www.freighter.app/) browser extension in your
**Windows** browser (Edge/Chrome/Firefox). It talks to the network over RPC, so
it works fine alongside WSL2 — you only need it for frontend interaction.

## 3. Clone and build

Keep your code on the **Linux** filesystem (`/home/you/...`), not the Windows
mount (`/mnt/c/...`) — builds are dramatically faster and case-sensitivity issues
disappear.

```bash
git clone https://github.com/astera-hq/Astera.git
cd Astera
cargo build --target wasm32-unknown-unknown --release
```

## 4. Run the stack

The easiest path is Docker Compose, which runs inside WSL2's own Docker
engine. Install [Docker Desktop](https://www.docker.com/products/docker-desktop/),
enable the **WSL2 backend** (Settings → General → "Use the WSL 2 based engine"),
and grant it access to your Ubuntu distro (Settings → Resources → WSL
Integration).

Then, from the repo root inside Ubuntu:

```bash
docker compose up --build
```

The frontend is served at http://localhost:3000 (reachable from your Windows
browser). See [Local Development with Docker Compose](local-development.md) for
the full service layout and troubleshooting.

## 5. Common pitfalls

- **`cannot find wasm32-unknown-unknown` during build:** you skipped
  `rustup target add wasm32-unknown-unknown`. Re-run it.
- **Builds hang / extremely slow:** you cloned into `/mnt/c`. Move the repo to
  your Linux home directory.
- **`stellar: command not found`:** your shell didn't pick up cargo. Run
  `source "$HOME/.cargo/env"` or restart the terminal.
- **Docker command not found in Ubuntu:** Docker Desktop's WSL integration is
  off — enable it in Docker Desktop settings and restart the terminal.
