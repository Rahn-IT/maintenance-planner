-- Add migration script here
CREATE TABLE action_items (
    id TEXT PRIMARY KEY,
    order_index INTEGER NOT NULL,
    action_plan TEXT NOT NULL,
    name TEXT NOT NULL,
    FOREIGN KEY (action_plan) REFERENCES action_plans(id)
);
CREATE INDEX action_plan_items_idx ON action_items(action_plan);
CREATE INDEX action_plan_items_name_idx ON action_items(name);
