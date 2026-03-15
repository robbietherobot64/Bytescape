# Bytescape

**The Local-First Social Cloud.**

Bytescape is a unified social vault and communication client. It operates on a "Low Floor, High Ceiling" philosophy: simple enough for anyone to use as a universal social app, powerful enough to serve as a sovereign, self-hosted P2P node.

## Architecture: Engine & Lens

- **Bytescape Bridge (The Engine):** A headless local backend that manages your data "Vault", P2P networking (Iroh), and connections to external networks (Bluesky, ActivityPub).
- **Bytescape Browser (The Lens):** A beautiful, "Frutiger Aero" inspired interface that lets you browse, search, and interact with your unified social history.

> "Web2 Convenience, Web3 Privacy."

## 📚 Documentation

*   **[Technical Specification](./TECHNICAL_SPEC.md)**: The "Engine/Lens" design, Universal Container schema, P2P topology, data flows, and security model.
*   **[Roadmap](./ROADMAP.md)**: Our path to the Sovereign Cloud.

## 🚀 Getting Started

The project is split into the frontend (`bytescape browser`) and the backend bridge (`bytescape-bridge`).

### Prerequisites
*   **Rust** (v1.75+ or Latest Stable)
*   **Node.js** (v18+) & **pnpm**
*   **Linux dependencies** for Tauri (check Tauri docs)

### Installation

1.  Navigate to the project directory:
    ```bash
    cd bytescape
    ```
2.  Install dependencies:
    ```bash
    pnpm install
    ```
3.  Run the application in development mode:
    ```bash
    pnpm tauri dev
    ```

### Building

To build for production:
```bash
pnpm tauri build
```

## 🏗️ Tech Stack

*   **Frontend**: React, TypeScript, CSS (Aero Theme)
*   **Backend**: Rust, Tauri v2
*   **Networking**: [Iroh](https://iroh.computer)
*   **Database**: redb
*   **Identity**: Decentralized Identifier (DID)

## License

MIT
