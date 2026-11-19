-- Notes per task (docs/SPEC.md, 4.2 and 4.5): free text, changed only in the edit dialog, and
-- when it last changed (for the period of the notes view).
ALTER TABLE tasks
    ADD COLUMN notes            text,
    ADD COLUMN notes_changed_at timestamptz;
