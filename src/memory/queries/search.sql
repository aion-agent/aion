SELECT m.key, m.value
FROM memory_fts
JOIN memory m ON memory_fts.rowid = m.rowid
WHERE memory_fts MATCH ?1
ORDER BY rank