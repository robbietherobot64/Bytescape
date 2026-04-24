# Lesson 03: Async, Await, and Promises

## Why This Matters

This app is full of asynchronous operations:

1. command calls to backend
2. file and media loading
3. network and sync operations

If async is unclear, the app feels random.

## Promise Basics

A Promise is a value that will be available later.

```ts
const p: Promise<number> = Promise.resolve(42);
```

States:

1. pending
2. fulfilled
3. rejected

## Async/Await Syntax

```ts
async function getCount(): Promise<number> {
  const value = await Promise.resolve(5);
  return value;
}
```

Meaning:

1. async marks function as returning Promise.
2. await pauses inside function until Promise resolves.
3. return sends resolved value.

## Error Handling With try/catch

```ts
try {
  const files = await invoke('vault_list_files');
  console.log(files);
} catch (err) {
  console.error('failed', err);
}
```

Why this matters:

- Backend command calls can fail.
- Missing try/catch leads to unhandled rejections.

## Parallel Work With Promise.all

```ts
const results = await Promise.all([
  invoke('cmd_one'),
  invoke('cmd_two')
]);
```

Use this when tasks are independent.

## Bytescape Mapping

In ProfileScreen loadMedia you see async patterns:

1. await invoke('vault_list_files')
2. batched Promise.all for thumbnail requests
3. try/catch/finally for loading state and failure behavior

## Quick Check

1. Why does async function always return Promise?
2. What does finally do in try/catch/finally?
3. When should Promise.all be used?
