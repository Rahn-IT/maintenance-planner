-- Add migration script here
CREATE TABLE action_items (
    id BLOB PRIMARY KEY NOT NULL,
    order_index INTEGER NOT NULL,
    action_plan BLOB NOT NULL,
    action BLOB NOT NULL,
    FOREIGN KEY (action_plan) REFERENCES action_plans(id)
    FOREIGN KEY (action) REFERENCES actions(id)
);
CREATE INDEX action_plan_items_idx ON action_items(action_plan);
CREATE INDEX action_plan_items_action_idx ON action_items(action);
CREATE INDEX action_plan_item_idx ON actions(id);
