-- Add migration script here

CREATE TABLE action_plans (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL
);
