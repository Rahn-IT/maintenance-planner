-- Add migration script here
CREATE TABLE action_item_executions (
    id BLOB PRIMARY KEY,
    action_item BLOB NOT NULL,
    action_plan_execution BLOB NOT NULL,
    FOREIGN KEY (action_item) REFERENCES action_items(id),
    FOREIGN KEY (action_plan_execution) REFERENCES action_plan_executions(id)
);
CREATE INDEX action_item_idx ON action_item_executions(action_item);
CREATE INDEX action_plan_execution_items_idx ON action_item_executions(action_plan_execution);
