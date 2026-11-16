#!/usr/bin/env bash
# Start a throwaway PostgreSQL with the sample "shop" database (requires Docker).
#   ./scripts/demo-db.sh        start (port 55432, user postgres / password postgres)
#   ./scripts/demo-db.sh stop   remove it
set -euo pipefail
NAME=snaptable-demo
PORT=${SNAPTABLE_DEMO_PORT:-55432}

if [[ "${1:-}" == "stop" ]]; then
  docker rm -f "$NAME" >/dev/null && echo "Stopped $NAME"
  exit 0
fi

docker rm -f "$NAME" >/dev/null 2>&1 || true
docker run -d --name "$NAME" -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=shop \
  -p "$PORT:5432" postgres:17 >/dev/null
echo -n "Waiting for PostgreSQL"
until docker exec "$NAME" pg_isready -U postgres -d shop >/dev/null 2>&1; do echo -n "."; sleep 1; done
echo
docker exec -i "$NAME" psql -q -U postgres -d shop < "$(dirname "$0")/demo.sql"
cat <<MSG
Demo database ready. In SnapTable choose New Connection (⌘N) → PostgreSQL and paste:

  postgres://postgres:postgres@127.0.0.1:$PORT/shop

Stop it with: ./scripts/demo-db.sh stop
MSG
