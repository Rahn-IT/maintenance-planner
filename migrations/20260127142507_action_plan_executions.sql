-- Add migration script here
CREATE TABLE action_plan_executions (
    id TEXT PRIMARY KEY,
    action_plan TEXT NOT NULL,
    /* Unix timestamp */
    started INTEGER NOT NULL,
    /* Unix timestamp */
    finished INTEGER NOT NULL,
    FOREIGN KEY (action_plan) REFERENCES action_plans(id)
);
CREATE INDEX action_plan_executions_idx ON action_plan_executions(action_plan);
