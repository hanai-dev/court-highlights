#!/usr/bin/env sh
# Online-service image gate (HOST-container-image, HOST-container-nonroot, HOST-container-health):
#   base images pinned by digest · runs as non-root · declares a health check that turns healthy on a read-only root
#   filesystem against the test stack · no unpatched Critical/High finding (Trivy).
# Usage: tools/cloud-image-check.sh <image>   (the test stack's db, s3 and ses must be up; needs docker)
set -u
IMAGE=${1:?usage: tools/cloud-image-check.sh <image>}
TRIVY=aquasec/trivy:0.75.0
fail=0
ok() { echo "ok    $1"; }
bad() { echo "FAIL  $1"; fail=1; }

froms=$(grep -E '^FROM ' docker/cloud.Dockerfile)
if echo "$froms" | grep -qv '@sha256:'; then bad "a base image is not pinned by digest"; else ok "base images pinned by digest: $(echo "$froms" | sed 's/.*@sha256:\(............\).*/\1…/' | tr '\n' ' ')"; fi

user=$(docker image inspect "$IMAGE" --format '{{.Config.User}}')
case "$user" in "" | root | 0 | 0:0) bad "runs as root (User='$user')" ;; *) ok "runs as $user" ;; esac

health=$(docker image inspect "$IMAGE" --format '{{json .Config.Healthcheck.Test}}')
case "$health" in *healthcheck*) ok "health check declared: $health" ;; *) bad "no health check" ;; esac

echo "image $(docker image inspect "$IMAGE" --format '{{.Id}}') size $(docker image inspect "$IMAGE" --format '{{.Size}}') bytes"

# Run it read-only, as the platform would, against the test stack's database, store and mail stand-ins.
docker rm -f ch-cloud-image-check >/dev/null 2>&1
# Restarted on failure like the platform does: the object store may still be starting.
docker run -d --name ch-cloud-image-check --read-only --restart on-failure --network ch-test_default \
  -e DATABASE_URL=postgres://postgres:ch-test-only@db/ch -e CH_PUBLIC_URL=http://localhost:18091 \
  -e CH_S3_ENDPOINT=http://s3:8333 -e CH_S3_BUCKET=clips -e CH_S3_KEY=ch-test-access -e CH_S3_SECRET=ch-test-only-not-a-secret \
  -e CH_SES_ENDPOINT=http://ses:8025 -e CH_SES_KEY=ch-test-access -e CH_SES_SECRET=ch-test-only-not-a-secret -e CH_MAIL_FROM=videos@test.invalid \
  -e CH_EMAIL_PEPPER=ch-test-only-pepper-not-a-secret-0123456789 \
  -e CH_SECRET_KEY=0000000000000000000000000000000000000000000000000000000000000001 \
  -e CH_READY_TOKEN=ch-test-only-ready-token \
  --health-interval 3s --health-start-period 5s "$IMAGE" >/dev/null || bad "container did not start"
status=starting
for _ in $(seq 1 40); do
  status=$(docker inspect ch-cloud-image-check --format '{{.State.Health.Status}}' 2>/dev/null || echo missing)
  [ "$status" = healthy ] && break
  sleep 3
done
if [ "$status" = healthy ]; then ok "healthy on a read-only root filesystem"; else bad "health status $status"; docker logs --tail 20 ch-cloud-image-check; fi
docker rm -f ch-cloud-image-check >/dev/null 2>&1

echo
echo "# trivy ($TRIVY) image --scanners vuln,secret --severity HIGH,CRITICAL --ignore-unfixed"
MSYS_NO_PATHCONV=1 docker run --rm -v /var/run/docker.sock:/var/run/docker.sock "$TRIVY" image --quiet --scanners vuln,secret --severity HIGH,CRITICAL --ignore-unfixed --exit-code 1 "$IMAGE" || bad "unpatched Critical/High findings or a secret in the image"
echo
[ "$fail" -eq 0 ] && echo "CLOUD-IMAGE: PASS" || { echo "CLOUD-IMAGE: FAIL"; exit 1; }
