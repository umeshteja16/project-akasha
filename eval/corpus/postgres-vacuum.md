# PostgreSQL VACUUM and table bloat

PostgreSQL uses MVCC: an UPDATE writes a new row version and leaves the old one behind as a dead tuple. VACUUM reclaims the space of dead tuples so it can be reused; VACUUM FULL rewrites the whole table and takes an exclusive lock, so avoid it on busy tables.

Autovacuum runs automatically, but its defaults are tuned for small tables. For a large, frequently updated table lower `autovacuum_vacuum_scale_factor` (for example to 0.02) so it runs before bloat builds up.

Long-running transactions stop vacuum from removing rows that are still visible to them; check `pg_stat_activity` for sessions that stay "idle in transaction". Vacuum also freezes old transaction ids to prevent transaction id wraparound.
