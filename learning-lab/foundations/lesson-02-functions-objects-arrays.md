# Lesson 02: Functions, Objects, and Arrays

## Part A: Functions

Functions package reusable behavior.

```ts
function toDisplayName(handle: string): string {
  return '@' + handle;
}
```

Syntax dissection:

1. function keyword defines a named function.
2. handle: string is input parameter type.
3. : string after ) is return type.
4. return sends value back to caller.

## Part B: Object Basics

Objects group related data.

```ts
const user = {
  id: '123',
  handle: 'alice',
  isOnline: true,
};
```

Access values:

```ts
console.log(user.handle);
```

## Part C: Interfaces (Object Blueprints)

```ts
interface AuthStatus {
  did: string;
  handle: string;
  source: string;
}
```

Why interfaces matter:

1. They document expected fields.
2. They prevent shape mistakes at compile time.

## Part D: Array Patterns

```ts
const ids = files.map(f => f.id);
const images = files.filter(f => f.kind === 'Image');
const firstImage = images.find(f => f.thumbnail_hash);
```

Meaning:

1. map transforms each item.
2. filter keeps matching items.
3. find returns first match or undefined.

## Bytescape Mapping

In media loading flows you will repeatedly see:

1. Array filtering by kind.
2. Mapping file metadata to display data.
3. Looking up specific fields for conditional behavior.

## Quick Check

1. What is the difference between map and filter?
2. Why define an interface instead of using untyped objects everywhere?
3. What does find return when no item matches?
