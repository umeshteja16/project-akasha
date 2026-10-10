#!/usr/bin/env bash
# Back up Akasha: a Postgres dump (custom format, includes pgvector data) plus the
# local blob store. See docs/operations.md.
#
#   scripts/backup.sh [-d DATABASE_URL] [-s STORAGE_DIR] [-o OUT_DIR]
#
# Environment (flags win): DATABASE_URL, AKASHA_STORAGE_DIR (default ./storage),
# AKASHA_STORAGE_BACKEND (`s3` skips the blob step: back the bucket up with your
# provider's tools), AKASHA_BACKUP_DIR (default ./backups).
#
# Creates OUT_DIR/akasha-<UTC timestamp>/ with db.dump, storage.tar, SHA256SUMS and
# backup.info, and prints its path. Order matters: the database is dumped FIRST, the blobs
# copied second. Blobs are content-addressed, so a blob uploaded in between is merely
# unreferenced (harmless); the reverse order could leave rows pointing at missing blobs.
set -euo pipefail

usage() { sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

db_url="${DATABASE_URL:-}"
storage_dir="${AKASHA_STORAGE_DIR:-./storage}"
out_dir="${AKASHA_BACKUP_DIR:-./backups}"
while getopts "d:s:o:h" opt; do
  case "$opt" in
    d) db_url="$OPTARG" ;;
    s) storage_dir="$OPTARG" ;;
    o) out_dir="$OPTARG" ;;
    h) usage 0 ;;
    *) usage 2 >&2 ;;
  esac
done

[ -n "$db_url" ] || { echo "backup: set DATABASE_URL or pass -d" >&2; exit 2; }
command -v pg_dump >/dev/null || { echo "backup: pg_dump not found (install postgresql-client)" >&2; exit 2; }

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
dest="$out_dir/akasha-$stamp"
mkdir -p "$dest"
dest="$(cd "$dest" && pwd)"

echo "backup: dumping database -> $dest/db.dump"
pg_dump --dbname "$db_url" --format=custom --no-owner --no-privileges --file "$dest/db.dump"

have_storage=no
if [ "${AKASHA_STORAGE_BACKEND:-local}" = "s3" ]; then
  echo "backup: AKASHA_STORAGE_BACKEND=s3: not copying blobs; back up the bucket separately" >&2
elif [ -d "$storage_dir" ]; then
  echo "backup: archiving blobs from $storage_dir -> $dest/storage.tar"
  # `staging/` holds half-written uploads. tar exits 1 when a file changed while being
  # read (an upload racing the backup): the archive is still usable, so only fail on 2+.
  rc=0
  tar -C "$storage_dir" --exclude='./staging' -cf "$dest/storage.tar" . || rc=$?
  if [ "$rc" -gt 1 ]; then echo "backup: tar failed ($rc)" >&2; exit 1; fi
  [ "$rc" -eq 0 ] || echo "backup: warning: files changed during the copy (harmless, see docs)" >&2
  have_storage=yes
else
  echo "backup: storage dir $storage_dir does not exist; database only" >&2
fi

(
  cd "$dest"
  files=(db.dump)
  [ "$have_storage" = yes ] && files+=(storage.tar)
  sha256sum "${files[@]}" > SHA256SUMS
  {
    echo "created=$stamp"
    echo "storage=$have_storage"
    echo "pg_dump=$(pg_dump --version)"
  } > backup.info
)

echo "backup: done: $dest"
echo "$dest"
