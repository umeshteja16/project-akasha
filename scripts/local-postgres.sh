#!/usr/bin/env bash
# Start a throwaway Postgres without Docker (cloud agent sessions, CI-less boxes).
# Prefer `just db-up` (Docker) on a normal dev machine.
#
# Creates role/db `akasha`/`akasha` on localhost:5432. Idempotent.
set -euo pipefail

PGBIN="${PGBIN:-$(ls -d /usr/lib/postgresql/*/bin 2>/dev/null | sort -V | tail -1)}"
DATA="${PGDATA_DIR:-/var/lib/postgresql/akasha-dev}"
RUN_AS=()
[ "$(id -u)" = 0 ] && RUN_AS=(runuser -u postgres --)

if [ -z "$PGBIN" ]; then
  echo "no local postgres install found; use 'just db-up' (Docker) instead" >&2
  exit 1
fi

# pgvector: Docker/CI images ship it; the distro Postgres usually does not.
# Fast no-op when the extension is already installed.
PGMAJOR="$(basename "$(dirname "$PGBIN")")"
PGSHARE="/usr/share/postgresql/$PGMAJOR/extension"
install_pgvector() {
  [ -f "$PGSHARE/vector.control" ] && return 0
  echo "installing pgvector for postgres $PGMAJOR..."
  local SUDO=()
  [ "$(id -u)" != 0 ] && SUDO=(sudo)
  if command -v apt-get >/dev/null 2>&1; then
    if "${SUDO[@]}" apt-get install -y -qq "postgresql-$PGMAJOR-pgvector" >/dev/null 2>&1 \
      || { "${SUDO[@]}" apt-get update -qq >/dev/null 2>&1 \
        && "${SUDO[@]}" apt-get install -y -qq "postgresql-$PGMAJOR-pgvector" >/dev/null 2>&1; }; then
      [ -f "$PGSHARE/vector.control" ] && return 0
    fi
    # No package: build from source.
    "${SUDO[@]}" apt-get install -y -qq build-essential git "postgresql-server-dev-$PGMAJOR" >/dev/null 2>&1 || true
  fi
  local SRC PGCONFIG="$PGBIN/pg_config"
  [ -x "$PGCONFIG" ] || PGCONFIG="$(command -v pg_config)"
  SRC="$(mktemp -d)"
  git clone -q --depth 1 --branch "${PGVECTOR_VERSION:-v0.8.0}" https://github.com/pgvector/pgvector.git "$SRC"
  make -s -C "$SRC" PG_CONFIG="$PGCONFIG"
  "${SUDO[@]}" make -s -C "$SRC" PG_CONFIG="$PGCONFIG" install
  rm -rf "$SRC"
}
if ! install_pgvector || [ ! -f "$PGSHARE/vector.control" ]; then
  echo "warning: pgvector is not installed; migrations needing it will fail" >&2
fi

# pg_trgm and btree_gin (spelling suggestions) come with the contrib modules,
# which the distro server package ships.
for ext in pg_trgm btree_gin; do
  [ -f "$PGSHARE/$ext.control" ] || echo "warning: postgres contrib extension $ext is missing (install postgresql-contrib)" >&2
done

if "${RUN_AS[@]}" "$PGBIN/pg_ctl" -D "$DATA" status >/dev/null 2>&1; then
  echo "postgres already running ($DATA)"
else
  if [ ! -f "$DATA/PG_VERSION" ]; then
    mkdir -p "$DATA"
    [ "$(id -u)" = 0 ] && chown postgres "$DATA"
    # UTF-8 like the Docker image: full-text search and the spelling vocabulary
    # only recognise non-ASCII letters in a UTF-8 database.
    "${RUN_AS[@]}" "$PGBIN/initdb" -D "$DATA" -U postgres -A trust --encoding=UTF8 --locale=C.UTF-8 >/dev/null
  fi
  "${RUN_AS[@]}" "$PGBIN/pg_ctl" -D "$DATA" -o "-p 5432 -k /tmp" -l "$DATA/server.log" -w start >/dev/null
  echo "postgres started ($DATA)"
fi

psql -h localhost -U postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname='akasha'" | grep -q 1 \
  || psql -h localhost -U postgres -qc "CREATE ROLE akasha LOGIN SUPERUSER PASSWORD 'akasha'"
psql -h localhost -U postgres -tAc "SELECT 1 FROM pg_database WHERE datname='akasha'" | grep -q 1 \
  || psql -h localhost -U postgres -qc "CREATE DATABASE akasha OWNER akasha"
ENCODING="$(psql -h localhost -U postgres -tAc "SELECT pg_encoding_to_char(encoding) FROM pg_database WHERE datname='akasha'")"
if [ "$ENCODING" != "UTF8" ]; then
  echo "warning: database akasha is $ENCODING, not UTF8; non-ASCII words are not searchable." >&2
  echo "         Recreate the cluster: pg_ctl -D $DATA stop && rm -rf $DATA && $0" >&2
fi
echo "DATABASE_URL=postgres://akasha:akasha@localhost:5432/akasha"
