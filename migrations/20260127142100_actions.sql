-- Add migration script here
CREATE TABLE actions (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL
);
CREATE INDEX actions_idx ON actions(id);
CREATE INDEX actions_name_idx ON actions(name);
