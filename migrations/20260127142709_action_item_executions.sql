-- Add migration script here
CREATE TABLE action_item_executions (
    id TEXT PRIMARY KEY,
    action_item TEXT NOT NULL,
    order_index INTEGER NOT NULL,
    action_plan_execution TEXT NOT NULL,
    /* Unix timestamp */
    finished INTEGER NOT NULL,
    FOREIGN KEY (action_item) REFERENCES action_items(id),
    FOREIGN KEY (action_plan_execution) REFERENCES action_plan_executions(id)
);
CREATE INDEX action_item_idx ON action_item_executions(action_item);
CREATE INDEX action_plan_execution_items_idx ON action_item_executions(action_plan_execution);
