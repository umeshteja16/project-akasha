# Choosing PostgreSQL indexes

B-tree is the default and serves equality and range queries and ORDER BY. GIN indexes serve containment queries: arrays, jsonb and full-text search vectors. GiST and SP-GiST serve geometric data and nearest-neighbour searches.

A partial index (`CREATE INDEX ... WHERE status = 'pending'`) stays small when queries only ever look at a few rows. A multicolumn index helps queries that filter on its leading columns.

Always check a plan with `EXPLAIN (ANALYZE, BUFFERS)`: a sequential scan on a small table is fine, but on a large one it usually means a missing index or a predicate that cannot use one, such as a function applied to the indexed column. Build indexes on live tables with `CREATE INDEX CONCURRENTLY`.
