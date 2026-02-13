-- Add migration script here

CREATE TABLE action_plans (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    deleted_at INTEGER
);
CREATE INDEX action_plans_idx ON action_plans(id);
CREATE INDEX action_plans_deleted_at_idx ON action_plans(deleted_at);
