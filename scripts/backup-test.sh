#!/usr/bin/env bash
# Round-trip test for scripts/backup.sh and scripts/restore.sh (`just backup-test`).
# Needs DATABASE_URL (a role that may CREATE DATABASE and CREATE EXTENSION), the Postgres
# client tools and sqlx-cli. Creates two scratch databases and a temp dir, removes them after.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
: "${DATABASE_URL:?set DATABASE_URL}"
tmp="$(mktemp -d)"
suffix="$$_$(date +%s)"
src_db="akasha_bt_src_$suffix"
dst_db="akasha_bt_dst_$suffix"
# Same server and credentials, different database name (strip any ?query first).
base="${DATABASE_URL%%\?*}"
admin_url="${base%/*}/postgres"
url_for() { echo "${base%/*}/$1"; }

cleanup() {
  psql "$admin_url" -Xq -c "drop database if exists $src_db" -c "drop database if exists $dst_db" || true
  rm -rf "$tmp"
}
trap cleanup EXIT

psql "$admin_url" -Xq -c "create database $src_db" -c "create database $dst_db"
sqlx migrate run --source "$root/crates/db/migrations" --database-url "$(url_for "$src_db")" >/dev/null

hash="$(printf 'hello backup' | sha256sum | cut -d' ' -f1)"
psql "$(url_for "$src_db")" -Xq -v ON_ERROR_STOP=1 <<SQL
insert into users (id, email, password_hash) values ('00000000-0000-0000-0000-000000000001', 'bt@example.com', 'x');
insert into files (id, owner_id, original_name, content_hash, mime_type, size_bytes, status)
  values ('00000000-0000-0000-0000-0000000000f1', '00000000-0000-0000-0000-000000000001',
          'note.txt', '$hash', 'text/plain', 12, 'ready');
insert into file_chunks (file_id, owner_id, chunk_index, char_start, char_end, text, embedding)
  values ('00000000-0000-0000-0000-0000000000f1', '00000000-0000-0000-0000-000000000001',
          0, 0, 12, 'hello backup', (select array_agg(0.5)::vector from generate_series(1, 384)));
SQL

store="$tmp/storage"
mkdir -p "$store/blobs/${hash:0:2}/${hash:2:2}" "$store/staging"
printf 'hello backup' > "$store/blobs/${hash:0:2}/${hash:2:2}/$hash"
echo partial > "$store/staging/half-written"

backup="$("$root/scripts/backup.sh" -d "$(url_for "$src_db")" -s "$store" -o "$tmp/out" | tail -n 1)"
"$root/scripts/restore.sh" -d "$(url_for "$dst_db")" -s "$tmp/restored" "$backup"

q() { psql "$(url_for "$dst_db")" -XAtq -c "$1"; }
[ "$(q "select email from users")" = "bt@example.com" ] || { echo "FAIL: user row" >&2; exit 1; }
[ "$(q "select text from file_chunks")" = "hello backup" ] || { echo "FAIL: chunk row" >&2; exit 1; }
[ "$(q "select vector_dims(embedding) from file_chunks")" = "384" ] || { echo "FAIL: embedding" >&2; exit 1; }
# Nearest-neighbour search through the restored HNSW index.
[ "$(q "select count(*) from (select 1 from file_chunks order by embedding <=> (select embedding from file_chunks limit 1) limit 1) t")" = "1" ] \
  || { echo "FAIL: vector query" >&2; exit 1; }
[ "$(cat "$tmp/restored/blobs/${hash:0:2}/${hash:2:2}/$hash")" = "hello backup" ] || { echo "FAIL: blob" >&2; exit 1; }
[ ! -e "$tmp/restored/staging/half-written" ] || { echo "FAIL: staging was backed up" >&2; exit 1; }

# A second restore into the now non-empty database must refuse without -f, and work with it.
if "$root/scripts/restore.sh" -d "$(url_for "$dst_db")" -s "$tmp/restored" "$backup" 2>/dev/null; then
  echo "FAIL: restore overwrote a non-empty database without -f" >&2; exit 1
fi
"$root/scripts/restore.sh" -f -d "$(url_for "$dst_db")" -s "$tmp/restored" "$backup" >/dev/null

echo "backup-test: ok"
