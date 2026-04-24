# Exercise 01: TypeScript Core

Goal: verify variables, types, conditionals, arrays, and function basics.

## Part A: Variables and Types (3 points)

For each line, state the type and whether reassignment is allowed.

1. const title = 'Bytescape';
2. let count: number = 0;
3. const flags = [true, false, true];

## Part B: Optional and Nullable Fields (2 points)

Given this interface:

interface VaultFileLite {
  id: string;
  thumbnail_hash?: string | null;
}

Questions:

1. Name three valid values for thumbnail_hash at runtime.
2. Why is this safer than forcing thumbnail_hash: string always?

## Part C: Conditionals (2 points)

What does this return?

function classify(kind: 'Image' | 'Video' | 'Audio') {
  if (kind === 'Image' || kind === 'Video') return 'visual';
  return 'non-visual';
}

1. classify('Image')
2. classify('Audio')

## Part D: Arrays (3 points)

Given:

const files = [
  { id: 'a', kind: 'Image', size: 100 },
  { id: 'b', kind: 'Video', size: 200 },
  { id: 'c', kind: 'Audio', size: 300 },
];

Answer:

1. Write a filter expression for visual files.
2. Write a map expression for only ids.
3. Write a find expression for first file larger than 150.
