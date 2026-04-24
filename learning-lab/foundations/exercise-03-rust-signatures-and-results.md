# Exercise 03: Rust Signatures and Result Shapes

Goal: decode Rust command signatures and return paths.

## Part A: Signature Reading (4 points)

Read this shape:

#[tauri::command]
pub async fn demo(file_id: String) -> Result<Option<Vec<u8>>, String> {
  ...
}

Explain each item:

1. #[tauri::command]
2. pub
3. async fn
4. Result<Option<Vec<u8>>, String>

## Part B: Outcome Enumeration (3 points)

List all possible runtime outcomes for Result<Option<Vec<u8>>, String>.

## Part C: Early Return Operator (3 points)

Given:

let sync = sync_guard.as_ref()
  .ok_or_else(|| SomeError)?;

Questions:

1. What does ok_or_else do here?
2. What does ? do if the value is missing?
3. Why is this pattern useful in command handlers?
