# Week 2 Plan: Feed And State Architecture

## Objective
Understand how feed data moves through hooks, cache, and UI, then apply one small feed behavior fix safely.

## Lessons
1. Feed lifecycle fundamentals
   - File focus: bytescape/src/hooks/useUnifiedFeed.ts
   - Outcome: explain load, refresh, and pagination flow

2. State boundaries and tab persistence
   - File focus: bytescape/src/store/index.ts and bytescape/src/screens/FeedScreen.tsx
   - Outcome: explain where data is global vs local

3. SWR and stale data behavior
   - File focus: useUnifiedFeed and FeedCacheStore
   - Outcome: diagnose stale-vs-fresh rendering intentionally

## Application Lab
Pick one feed issue and run full debug loop:

1. reproduce
2. isolate path
3. patch minimally
4. verify with cargo/tsc/tests

## Definition Of Done
- You can explain feed data lifecycle from memory.
- You can identify where to patch feed behavior safely.
- You complete one validated feed-focused fix.
