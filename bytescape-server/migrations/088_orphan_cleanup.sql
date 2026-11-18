-- add-identity-realignment task 4.4: one-shot orphan cleanup.
--
-- Rows under a local minted actor (`direction = 'out'`) that map to no live
-- user and no retirement-registry entry were left behind by accounts that died
-- before 087's owner FK existed (the 2026-11-14 incident class). Deliberately
-- preserved: inbound (`direction = 'in'`) federated history of remote actors,
-- and every retirement-staging row (registry-owned — retractions in flight and
-- superseded-actor payloads stay dereferenceable per actors/spec.md).
-- Idempotent: the migration runner re-runs every migration at boot.

DELETE FROM ap_delivery_queue q
WHERE EXISTS (
    SELECT 1 FROM ap_activities a
    WHERE a.id = q.activity_id
      AND a.direction = 'out'
      AND a.user_id IS NULL
      AND NOT EXISTS (SELECT 1 FROM users u WHERE u.ap_actor_id = a.actor_id)
      AND NOT EXISTS (SELECT 1 FROM retired_ap_actors r WHERE r.actor_id = a.actor_id)
);

DELETE FROM ap_activities a
WHERE a.direction = 'out'
  AND a.user_id IS NULL
  AND NOT EXISTS (SELECT 1 FROM users u WHERE u.ap_actor_id = a.actor_id)
  AND NOT EXISTS (SELECT 1 FROM retired_ap_actors r WHERE r.actor_id = a.actor_id);
