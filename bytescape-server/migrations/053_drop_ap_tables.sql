-- Migration 053: Drop ActivityPub federation tables (Sprint 4 AP subtraction).
-- These tables were part of the AP federation stack which has been removed.
-- ap_actors: AP actor keys and metadata
-- delivery_queue: async AP activity delivery queue
DROP TABLE IF EXISTS ap_actors;
DROP TABLE IF EXISTS delivery_queue;
