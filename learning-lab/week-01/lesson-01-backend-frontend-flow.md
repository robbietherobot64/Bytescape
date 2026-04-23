# Lesson 1: Frontend -> Backend Flow

## Why This Matters
Most bugfixes become easier once you can trace one user action through the full stack.

## Learning Objective
Trace one feed or profile action from React to Rust and back.

## Files To Study
- bytescape/src/App.tsx
- bytescape/src/providers/DataProvider.ts
- bytescape/src/providers/TauriDataProvider.ts
- bytescape/src-tauri/src/lib.rs
- bytescape/src-tauri/src/commands/

## Mental Model
1. UI interaction happens in a screen or component.
2. Component calls a provider method.
3. Tauri provider calls invoke("command_name", params).
4. Rust command is registered in lib.rs.
5. Command function executes in commands module and accesses DB/connectors.
6. Rust returns serializable data.
7. Frontend receives typed result and renders UI.

## Guided Exercise
Choose one action (example: loading profile media):
1. Find where the action starts in frontend screen code.
2. Find the provider method used.
3. Find the invoke command name.
4. Find where that command is registered in lib.rs.
5. Find the command implementation file.
6. Describe what data is returned and where it is rendered.

## Checkpoint Questions
- Which exact invoke command name is called?
- Which Rust file implements it?
- What TypeScript type is expected in the UI?

## Exit Ticket
Write a 6-8 line summary of the path you traced in progress-tracker.md.
