# Lesson 01: Types, Variables, and Control Flow

## Part A: Variables

A variable is a named box that stores a value.

TypeScript examples:

```ts
const appName: string = 'Bytescape';
let mediaCount: number = 0;
const isLoading: boolean = true;
```

Rules:

1. const means the variable binding does not get reassigned.
2. let means reassignment is allowed.
3. Type annotations help catch mistakes before runtime.

## Part B: Basic Types

Common types you will see constantly:

1. string
2. number
3. boolean
4. null
5. undefined
6. arrays like string[]
7. object shapes via interfaces

Bytescape example shape:

```ts
interface VaultFile {
  id: string;
  kind: 'Image' | 'Video' | 'Audio' | 'Document' | 'Archive' | 'Other';
  thumbnail_hash?: string | null;
}
```

Meaning:

1. id must be a string.
2. kind must be one of listed string literals.
3. thumbnail_hash may be missing or null.

## Part C: Conditionals

Conditionals let code branch.

```ts
if (file.kind === 'Image') {
  // image logic
} else {
  // non-image logic
}
```

Bytescape real pattern:

- If kind is Image and thumbnail exists, request thumbnail.
- Otherwise show placeholder behavior.

## Part D: Loops and Filtering

Loop example:

```ts
for (let i = 0; i < items.length; i += 1) {
  console.log(items[i]);
}
```

Filter example:

```ts
const visual = files.filter(f => f.kind === 'Image' || f.kind === 'Video');
```

Explanation:

1. filter returns a new array.
2. Arrow function returns true/false.
3. Only matching items remain.

## Quick Check

1. Why would thumbnail_hash be optional?
2. What is the difference between const and let?
3. What does files.filter(...) return?
