CREATE TABLE tags (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL
);

CREATE UNIQUE INDEX tags_name_unique_idx ON tags(name COLLATE NOCASE);
CREATE INDEX tags_name_idx ON tags(name);

CREATE TABLE action_plan_tags (
    action_plan BLOB NOT NULL,
    tag BLOB NOT NULL,
    PRIMARY KEY (action_plan, tag)
);

CREATE INDEX action_plan_tags_action_plan_idx ON action_plan_tags(action_plan);
CREATE INDEX action_plan_tags_tag_idx ON action_plan_tags(tag);
