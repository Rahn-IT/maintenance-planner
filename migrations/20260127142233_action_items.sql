-- Add migration script here
CREATE TABLE action_items (
    id BLOB PRIMARY KEY,
    action_plan BLOB NOT NULL,
    name TEXT NOT NULL,
    FOREIGN KEY (action_plan) REFERENCES action_plans(id)
);
CREATE INDEX action_plan_items_idx ON action_items(action_plan);
CREATE INDEX action_plan_items_name_idx ON action_items(name);
