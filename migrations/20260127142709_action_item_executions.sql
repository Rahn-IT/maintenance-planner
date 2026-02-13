-- Add migration script here
CREATE TABLE action_item_executions (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL,
    order_index INTEGER NOT NULL,
    action_plan_execution TEXT NOT NULL,
    /* Unix timestamp */
    finished INTEGER,
    FOREIGN KEY (action) REFERENCES actions(id),
    FOREIGN KEY (action_plan_execution) REFERENCES action_plan_executions(id)
);
CREATE INDEX action_execution_idx ON action_item_executions(action);
CREATE INDEX action_plan_execution_items_idx ON action_item_executions(action_plan_execution);
