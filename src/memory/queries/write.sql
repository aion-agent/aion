INSERT INTO memory (key, value, scope)
VALUES (?1, ?2, ?3)
ON CONFLICT(key) DO UPDATE SET
    value = excluded.value,
    updated_at = CURRENT_TIMESTAMP