# Postmortem: API outage on 14 March

**Impact:** the public API returned errors for 47 minutes; about 30% of requests failed.

**Timeline:** a scheduled database failover promoted the replica at 09:12. Application servers kept connections to the old primary, and the connection pool filled with broken connections. New requests waited for a free connection until they timed out.

**Root cause:** the pool had no health check and no maximum connection lifetime, so dead connections were never replaced.

**Action items:**
1. Enable connection validation and a 30-minute max lifetime in the pool.
2. Add an alert on pool wait time.
3. Rehearse failover in staging every quarter.
