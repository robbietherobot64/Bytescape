# Lesson 04: Debugging and Testing Basics

## Part A: Debugging Mindset

Do not start by changing code.

Use this sequence:

1. Reproduce the issue clearly.
2. Write expected vs actual behavior.
3. Isolate where behavior diverges.
4. Form one hypothesis.
5. Test hypothesis with minimal instrumentation.
6. Patch only after evidence.

## Part B: Useful Debug Tools

1. console.log for short-lived tracing
2. TypeScript compiler errors for shape mismatches
3. test failures for behavior mismatches
4. targeted grep/search for call chains

## Part C: Basic Testing Vocabulary

- Unit test: tests one small behavior.
- Integration test: tests component interactions.
- Assertion: expected condition in test.

## Part D: Project Commands You Will Use Constantly

Run from /home/korendian/Bytescape/bytescape:

```bash
(cd src-tauri && cargo check)
npx tsc --noEmit
pnpm test -- --run
```

Why each one exists:

1. cargo check: Rust compile safety
2. tsc --noEmit: TypeScript compile safety
3. pnpm test: behavior safety

## Part E: Minimal Logging Pattern

Good logging:

1. include ids and branch info
2. keep temporary logs scoped
3. remove logs after root cause is found

Bad logging:

1. noisy logs everywhere
2. logs with no labels
3. logs left in final patch without reason

## Quick Check

1. Why is reproduce-first important?
2. Which command catches TypeScript contract mistakes?
3. Which command checks Rust side compile issues?
