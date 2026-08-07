#!/bin/sh
set -eu

backup_root="${REGISTRY_BACKUP_DIR:-/data/cellscript-registry/backups}"
postgres_container="${REGISTRY_POSTGRES_CONTAINER:-cellscript-registry-postgres-1}"
objects_volume="${REGISTRY_OBJECTS_VOLUME:-cellscript-registry_registry-objects}"
retention_days="${REGISTRY_BACKUP_RETENTION_DAYS:-7}"

case "$backup_root" in
  /*) ;;
  *)
    echo "REGISTRY_BACKUP_DIR must be an absolute path" >&2
    exit 2
    ;;
esac

if [ "$backup_root" = "/" ]; then
  echo "REGISTRY_BACKUP_DIR must not be the filesystem root" >&2
  exit 2
fi

case "$retention_days" in
  ''|*[!0-9]*)
    echo "REGISTRY_BACKUP_RETENTION_DAYS must be an integer" >&2
    exit 2
    ;;
esac

if [ "$retention_days" -lt 1 ] || [ "$retention_days" -gt 365 ]; then
  echo "REGISTRY_BACKUP_RETENTION_DAYS must be between 1 and 365" >&2
  exit 2
fi

command -v docker >/dev/null 2>&1 || {
  echo "docker is required" >&2
  exit 2
}

install -d -m 750 "$backup_root"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
final_dir="$backup_root/$stamp"

if [ -e "$final_dir" ]; then
  echo "backup already exists: $final_dir" >&2
  exit 2
fi

temporary="$(mktemp -d "$backup_root/.tmp-$stamp.XXXXXX")"
cleanup() {
  if [ -n "${temporary:-}" ] && [ -d "$temporary" ]; then
    rm -rf -- "$temporary"
  fi
}
trap cleanup EXIT HUP INT TERM

docker exec "$postgres_container" pg_dump \
  --username cellscript_registry \
  --dbname cellscript_registry \
  --format custom \
  --no-owner \
  --no-privileges > "$temporary/postgres.dump"

docker run --rm \
  --network none \
  --read-only \
  --security-opt no-new-privileges:true \
  --volume "$objects_volume:/objects:ro" \
  --volume "$temporary:/backup" \
  alpine:3.22 \
  tar -czf /backup/objects.tar.gz -C /objects .

docker inspect --format '{{.Image}}' "$postgres_container" > "$temporary/postgres-image.txt"
(
  cd "$temporary"
  sha256sum postgres.dump objects.tar.gz postgres-image.txt > SHA256SUMS
)

chmod 640 "$temporary"/*
mv "$temporary" "$final_dir"
temporary=""

find "$backup_root" \
  -mindepth 1 \
  -maxdepth 1 \
  -type d \
  -name '20??????T??????Z' \
  -mtime "+$retention_days" \
  -exec rm -rf -- {} +

echo "$final_dir"
