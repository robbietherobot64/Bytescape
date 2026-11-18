//! Per-account PDS write quiesce (review #2 R2-3).
//!
//! The did:web→did:plc re-identify walks an account's repo OUTSIDE any
//! transaction (`pds::reidentify`), so a user write landing between walk
//! start and cutover would be destroyed by the cutover (its index/firehose/
//! root rows replaced with the walk's snapshot). While the migration holds
//! [`hold`] for an account, its record write paths refuse writes LOUDLY —
//! a transient Conflict error, never silent loss.
//!
//! Mechanism tradeoff (review #2 R2-3, documented in the OpenSpec change):
//! a per-account in-memory lock checked at the three `*_record_internal`
//! seams (every PDS mutation funnels through them) was chosen over
//! re-walk-and-reconcile (a second walk window to get wrong) or
//! walk-inside-the-cutover-transaction (the walk is I/O over many blocks and
//! would hold a DB transaction open for its duration). In-memory is sound
//! because the migration runner is a single boot-gated task in this process;
//! a stuck lock fails one account's writes loudly rather than losing them.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};

static QUIESCED: LazyLock<Mutex<HashMap<String, u32>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn set() -> MutexGuard<'static, HashMap<String, u32>> {
    QUIESCED.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// RAII hold of one account's write quiesce ([`hold`]'s return). Releases on
/// drop — including on the error paths a migration takes out of its window.
/// Nested holds are refcounted: the account stays quiesced until its LAST
/// guard drops.
pub(crate) struct QuiesceGuard {
    did: String,
}

impl Drop for QuiesceGuard {
    fn drop(&mut self) {
        let mut map = set();
        if let Some(count) = map.get_mut(&self.did) {
            *count -= 1;
            if *count == 0 {
                map.remove(&self.did);
            }
        }
    }
}

/// Quiesce the account whose repo currently lives at `did` (the migration's
/// OLD did — the identity racing writers target during the walk→cutover
/// window). Nested holds of one DID refcount (the account re-enables writes
/// only when the last guard drops).
pub(crate) fn hold(did: &str) -> QuiesceGuard {
    *set().entry(did.to_string()).or_insert(0) += 1;
    QuiesceGuard {
        did: did.to_string(),
    }
}

/// Refuse a record write while `did` is migrating (R2-3). Callers surface
/// the error as `AppError::Conflict` — the write fails visibly and can be
/// retried after the cutover, instead of vanishing inside the walk window.
pub(crate) fn check_write(did: &str) -> Result<(), String> {
    if set().contains_key(did) {
        return Err(format!(
            "account {did} is mid-identity-migration — retry in a moment"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lock is per DID and RAII-released: held → refused, released →
    /// allowed, and a second hold does not leak past its guard.
    #[test]
    fn held_account_writes_refused_until_release() {
        let did = format!("did:web:quiesce-{}", std::process::id());
        {
            let _guard = hold(&did);
            assert!(check_write(&did).is_err(), "held DID must refuse writes");
            assert!(
                check_write("did:web:someone-else").is_ok(),
                "other accounts are unaffected"
            );
            let _second = hold(&did);
            drop(_second);
            assert!(
                check_write(&did).is_err(),
                "a nested hold must not release the outer one early"
            );
        }
        assert!(check_write(&did).is_ok(), "release must re-enable writes");
    }
}
