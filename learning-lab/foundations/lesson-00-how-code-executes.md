# Lesson 00: How Code Executes

## Why This Comes First

Most beginners are not blocked by syntax. They are blocked by execution model confusion.

If you know what runs first, what waits, and what returns data, everything else becomes easier.

## Core Mental Model

Think in three questions:

1. Who calls this code?
2. What input does it receive?
3. What output does it return?

## Simple Example

```ts
function add(a: number, b: number): number {
  return a + b;
}

const result = add(2, 3);
```

Execution order:

1. Function add is defined.
2. add(2, 3) is called.
3. a=2, b=3.
4. return sends 5 back.
5. result stores 5.

## Bytescape Mapping Example

In bytescape/src/screens/ProfileScreen.tsx, loadMedia calls commands:

1. invoke('vault_list_files')
2. invoke('vault_get_thumbnail', { fileId })

Same mental model:

1. Caller: loadMedia.
2. Input: command name and params.
3. Output: typed data returned from backend.

## Vocabulary

- Call site: the place that invokes a function.
- Return value: data sent back from a function.
- Side effect: code that changes state or external systems.

## Quick Check

Answer in your notes:

1. What is the caller in the add example?
2. What is the return value?
3. In loadMedia, what data is returned by vault_list_files?
