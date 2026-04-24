# Exercise 02: Async, Await, and Error Flow

Goal: confirm async control flow understanding.

## Part A: Async Semantics (3 points)

Answer in one sentence each:

1. What does async guarantee about return type?
2. What does await do inside an async function?
3. What happens if an awaited Promise rejects and there is no catch?

## Part B: try/catch/finally Reasoning (3 points)

Given pattern:

setLoading(true);
try {
  const data = await fetchData();
  setItems(data);
} catch {
  setItems([]);
} finally {
  setLoading(false);
}

Questions:

1. Why setItems([]) in catch?
2. Why setLoading(false) in finally instead of try only?
3. What bug appears if finally is removed?

## Part C: Sequential vs Parallel (4 points)

You need thumbnails for 10 files.

1. Explain when Promise.all is better than awaiting each file one by one.
2. Explain one risk of firing too many Promise requests at once.
3. Why is batching (for example groups of 10) a good compromise?
4. In your own words, what does this mean:
   await Promise.all(batch.map(async (f) => { ... }))
