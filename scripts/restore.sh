#!/usr/bin/env bash
# Restore an Akasha backup made by scripts/backup.sh. See docs/operations.md.
#
#   scripts/restore.sh [-d DATABASE_URL] [-s STORAGE_DIR] [-f] BACKUP_DIR
#
# Environment (flags win): DATABASE_URL, AKASHA_STORAGE_DIR (default ./storage).
# The target database must already exist (e.g. `createdb akasha`) and be EMPTY, and the
# storage directory must be empty or missing; -f replaces what is there (drops existing
# objects first). Stop Akasha before restoring. The role needs permission to
# CREATE EXTENSION vector (the dump recreates it); the Docker pgvector image allows it.
set -euo pipefail

usage() { sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

db_url="${DATABASE_URL:-}"
storage_dir="${AKASHA_STORAGE_DIR:-./storage}"
force=no
while getopts "d:s:fh" opt; do
  case "$opt" in
    d) db_url="$OPTARG" ;;
    s) storage_dir="$OPTARG" ;;
    f) force=yes ;;
    h) usage 0 ;;
    *) usage 2 >&2 ;;
  esac
done
shift $((OPTIND - 1))
[ $# -eq 1 ] || usage 2 >&2
backup="$1"

[ -n "$db_url" ] || { echo "restore: set DATABASE_URL or pass -d" >&2; exit 2; }
[ -f "$backup/db.dump" ] || { echo "restore: $backup/db.dump not found" >&2; exit 2; }
command -v pg_restore >/dev/null || { echo "restore: pg_restore not found" >&2; exit 2; }
command -v psql >/dev/null || { echo "restore: psql not found" >&2; exit 2; }

echo "restore: verifying checksums"
(cd "$backup" && sha256sum --check --quiet SHA256SUMS)

tables="$(psql "$db_url" -XAtq -c "select count(*) from pg_tables where schemaname = 'public'")"
pg_args=(--no-owner --no-privileges --exit-on-error)
if [ "$tables" != "0" ]; then
  if [ "$force" != yes ]; then
    echo "restore: target database is not empty ($tables tables); use -f to replace it" >&2
    exit 1
  fi
  pg_args+=(--clean --if-exists)
fi

restore_storage=no
if [ -f "$backup/storage.tar" ]; then
  restore_storage=yes
  if [ -d "$storage_dir" ] && [ -n "$(ls -A "$storage_dir")" ] && [ "$force" != yes ]; then
    echo "restore: storage dir $storage_dir is not empty; use -f to merge into it" >&2
    exit 1
  fi
fi

echo "restore: loading database"
pg_restore --dbname "$db_url" "${pg_args[@]}" "$backup/db.dump"

if [ "$restore_storage" = yes ]; then
  echo "restore: extracting blobs into $storage_dir"
  mkdir -p "$storage_dir"
  tar -C "$storage_dir" -xf "$backup/storage.tar"
else
  echo "restore: no storage.tar in the backup (S3 backend?): restore the bucket separately" >&2
fi

echo "restore: done. Start Akasha; it applies any newer migrations on start."
