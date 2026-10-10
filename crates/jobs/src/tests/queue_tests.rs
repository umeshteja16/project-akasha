//! Queue operations: enqueue, dedupe, claim, reclaim, schedules, pruning.

use std::{collections::HashSet, time::Duration};

use sqlx::PgPool;

use super::*;

#[sqlx::test(migrator = "MIGRATOR")]
async fn enqueue_is_atomic_with_the_transaction(pool: PgPool) {
    let mut tx = pool.begin().await.expect("tx");
    let id = enqueue(&mut tx, &Echo { n: 1 }).await.expect("enqueue");
    tx.rollback().await.expect("rollback");
    let id = id.expect("id");
    assert!(queue::get(&pool, id).await.expect("get").is_none());

    let mut tx = pool.begin().await.expect("tx");
    let id = enqueue(&mut tx, &Echo { n: 1 }).await.expect("enqueue");
    tx.commit().await.expect("commit");
    assert_eq!(status(&pool, id.expect("id")).await.status, "queued");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn dedupe_applies_only_while_queued(pool: PgPool) {
    let first = put(&pool, &Echo { n: 7 }).await;
    assert!(first.is_some());
    assert_eq!(put(&pool, &Echo { n: 7 }).await, None, "already queued");
    assert!(put(&pool, &Echo { n: 8 }).await.is_some(), "other key");

    let claimed = queue::claim(&pool, "w", &[Echo::KIND.into()], 10)
        .await
        .expect("claim");
    assert_eq!(claimed.len(), 2);
    assert!(
        put(&pool, &Echo { n: 7 }).await.is_some(),
        "a running job does not block newer intent"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_claims_never_share_a_job(pool: PgPool) {
    const JOBS: u32 = 200;
    for n in 0..JOBS {
        put(&pool, &Echo { n }).await;
    }
    let mut handles = Vec::new();
    for w in 0..8 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            let mut mine = Vec::new();
            loop {
                let batch = queue::claim(&pool, &format!("w{w}"), &[Echo::KIND.into()], 5)
                    .await
                    .expect("claim");
                if batch.is_empty() {
                    break mine;
                }
                mine.extend(batch.into_iter().map(|j| j.id));
            }
        }));
    }
    let mut seen = HashSet::new();
    let mut total = 0;
    for handle in handles {
        for id in handle.await.expect("task") {
            total += 1;
            assert!(seen.insert(id), "job {id} claimed twice");
        }
    }
    assert_eq!(total, JOBS as usize);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn lost_workers_jobs_are_reclaimed(pool: PgPool) {
    let id = put(&pool, &Echo { n: 1 }).await.expect("id");
    let kinds = [Echo::KIND.to_owned()];
    let crashed = queue::claim(&pool, "crashed", &kinds, 1)
        .await
        .expect("claim");
    assert_eq!(crashed.len(), 1);
    assert_eq!(
        queue::reap_stale(&pool, Duration::from_secs(60))
            .await
            .expect("reap"),
        0
    );

    sqlx::query("UPDATE jobs SET locked_at = now() - interval '10 minutes' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("age heartbeat");
    assert_eq!(
        queue::reap_stale(&pool, Duration::from_secs(60))
            .await
            .expect("reap"),
        1
    );
    assert_eq!(status(&pool, id).await.status, "failed");

    let again = queue::claim(&pool, "healthy", &kinds, 1)
        .await
        .expect("claim");
    assert_eq!(again.first().map(|j| (j.id, j.attempts)), Some((id, 2)));
    assert!(
        !queue::complete(&pool, "crashed", &crashed[0])
            .await
            .expect("complete"),
        "the old owner can no longer finish it"
    );
    assert_eq!(
        queue::fail(&pool, "crashed", &crashed[0], "late", true)
            .await
            .expect("fail"),
        Failed::Lost
    );
    assert!(
        queue::complete(&pool, "healthy", &again[0])
            .await
            .expect("complete")
    );
    assert_eq!(status(&pool, id).await.status, "succeeded");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn reaping_a_job_without_attempts_left_kills_it(pool: PgPool) {
    let id = put(&pool, &Flaky).await.expect("id");
    sqlx::query(
        "UPDATE jobs SET status = 'running', attempts = 3, locked_by = 'gone',
         locked_at = now() - interval '1 hour' WHERE id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .expect("simulate");
    queue::reap_stale(&pool, Duration::from_secs(60))
        .await
        .expect("reap");
    assert_eq!(status(&pool, id).await.status, "dead");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn unregistered_kinds_stay_queued(pool: PgPool) {
    let ctx = Ctx::default();
    let id = put(&pool, &Unhandled).await.expect("id");
    assert_eq!(worker(&pool, &ctx).run_until_idle().await.expect("run"), 0);
    assert_eq!(status(&pool, id).await.status, "queued");
    let known = registry().kinds();
    let waiting = queue::unhandled_kinds(&pool, &known).await.expect("count");
    assert_eq!(waiting, vec![(Unhandled::KIND.to_owned(), 1)]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn schedules_fire_once_per_interval(pool: PgPool) {
    let every =
        Schedule::new("echo-hourly", Duration::from_secs(3600), &Echo { n: 0 }).expect("schedule");
    schedule::upsert(&pool, std::slice::from_ref(&every))
        .await
        .expect("upsert");
    let names = ["echo-hourly".to_owned()];
    assert_eq!(schedule::tick(&pool, &names).await.expect("tick"), 1);
    assert_eq!(
        schedule::tick(&pool, &names).await.expect("tick"),
        0,
        "not due again"
    );
    schedule::upsert(&pool, &[every])
        .await
        .expect("restart keeps next_run_at");
    assert_eq!(schedule::tick(&pool, &names).await.expect("tick"), 0);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'test_echo'")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn prune_removes_old_finished_jobs(pool: PgPool) {
    let ctx = Ctx::default();
    let id = put(&pool, &Echo { n: 1 }).await.expect("id");
    worker(&pool, &ctx).run_until_idle().await.expect("run");
    let week = Duration::from_secs(7 * 86_400);
    assert_eq!(
        queue::prune_finished(&pool, week, week)
            .await
            .expect("prune"),
        0
    );
    assert_eq!(
        queue::prune_finished(&pool, Duration::ZERO, week)
            .await
            .expect("prune"),
        1
    );
    assert!(queue::get(&pool, id).await.expect("get").is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn running_jobs_report_progress(pool: PgPool) {
    let id = put(&pool, &Echo { n: 7 }).await.expect("id");
    // Outside a worker: nothing to report to.
    queue::report_progress(&pool, 0.5).await;
    assert_eq!(status(&pool, id).await.progress, None);

    let claimed = queue::claim(&pool, "w", &[Echo::KIND.into()], 1)
        .await
        .expect("claim");
    let attempt = crate::Attempt {
        job_id: id,
        number: claimed[0].attempts,
        max: claimed[0].max_attempts,
    };
    attempt
        .scope(async {
            queue::report_progress(&pool, 0.25).await;
            queue::report_progress(&pool, 7.0).await; // clamped
        })
        .await;
    assert_eq!(status(&pool, id).await.progress, Some(1.0));

    // A new attempt starts from scratch.
    sqlx::query("UPDATE jobs SET status = 'failed', run_at = now(), locked_at = NULL, locked_by = NULL WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("fail");
    queue::claim(&pool, "w", &[Echo::KIND.into()], 1)
        .await
        .expect("claim");
    assert_eq!(status(&pool, id).await.progress, None);
}
