# Bytescape

Bytescape is a local-first, decentralized social vault that merges the real-time utility of modern chat applications with the asynchronous reliability of email-style threading.

## Technology Stack

- **Frontend**: React + TypeScript + Vite
- **Backend**: Rust + Tauri + Iroh
- **Data**: Redb (Embedded KV store)

## Getting Started

### Prerequisites

- Node.js (v18+)
- Rust (v1.75+)
- Linux dependencies for Tauri (check Tauri docs)

### Development

1. Install dependencies:
   ```bash
   cd bytescape
   pnpm install
   ```

2. Run in development mode:
   ```bash
   pnpm tauri dev
   ```

### Building

To build for production:
```bash
pnpm tauri build
```
