-- Tags are lower case from clockopus 0.2 on (normalize_tags). Existing data follows: tag names
-- and the tag lists of tasks in lower case, duplicates that differed only in case merged (the
-- first occurrence wins, order kept). The unique index on lower(name) per user means the tags
-- table has no such duplicates.

UPDATE tags SET name = lower(name) WHERE name <> lower(name);

UPDATE tasks
SET tags = coalesce((
        SELECT array_agg(l ORDER BY first)
        FROM (
            SELECT lower(t) AS l, min(i) AS first
            FROM unnest(tasks.tags) WITH ORDINALITY AS u(t, i)
            GROUP BY lower(t)
        ) AS lowered
    ), '{}')
WHERE EXISTS (SELECT 1 FROM unnest(tasks.tags) AS t WHERE t <> lower(t));

-- Running servers cache a user's data per version; make them reload.
UPDATE users SET version = version + 1;
