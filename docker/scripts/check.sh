#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

scripts=()
while IFS= read -r -d '' script; do
    [[ -x $script ]] || { printf 'FAIL: not executable: %s\n' "$script" >&2; exit 1; }
    case "$(head -n 1 "$script")" in
        *bash*) bash -n "$script" ;;
        *) sh -n "$script" ;;
    esac
    scripts+=("$script")
done < <(find scripts root/usr/local root/etc/s6-overlay -type f \( -name '*.sh' -o -name '3mf-*' -o -name run \) -print0)
for script in root/usr/local/bin/3mf-katalog root/usr/local/libexec/3mf-{auth,filesystem,init,session,supervise,healthcheck} root/etc/s6-overlay/s6-rc.d/svc-3mf/run root/etc/s6-overlay/s6-rc.d/init-3mf/up; do
    [[ -x $script ]] || { printf 'FAIL: missing executable: %s\n' "$script" >&2; exit 1; }
done
printf 'OK: shell syntax and executable permissions\n'
if command -v shellcheck >/dev/null 2>&1; then
    shellcheck "${scripts[@]}"
    printf 'OK: shellcheck\n'
else
    printf 'SKIP: shellcheck unavailable\n'
fi
if command -v hadolint >/dev/null 2>&1; then
    # The base distro supplies package versions; BASE_IMAGE can be pinned externally.
    hadolint --ignore DL3008 Dockerfile
    printf 'OK: hadolint\n'
else
    printf 'SKIP: hadolint unavailable; checking Dockerfile invariants\n'
fi
for expected in 'services:' 'build:' 'image: ghcr.io/bexxs75/3mf-katalog-manager:${IMAGE_TAG:-latest}' 'PUID:' 'PGID:' 'TZ:' 'UMASK:' '127.0.0.1:3000:3000' '127.0.0.1:3001:3001' './config:/config' '${MODELS_DIR:-./models}:/models' '/models2' 'shm_size: "1gb"' 'restart: unless-stopped' 'stop_grace_period: 30s' 'security_opt: [no-new-privileges:true]' 'Healthcheck inherited from the image'; do
    grep -Fq "$expected" docker-compose.yml
done
if grep -Eq '^ *healthcheck:|privileged:|docker.sock|cap_add:' docker-compose.yml; then
    printf 'FAIL: forbidden setting in docker-compose.yml\n' >&2
    exit 1
fi
if command -v python3 >/dev/null 2>&1 && python3 -c 'import yaml' 2>/dev/null; then
    python3 - <<'PY'
import yaml
with open("docker-compose.yml", encoding="utf-8") as stream:
    service = yaml.safe_load(stream)["services"]["3mf-katalog"]
assert service["build"]["context"] == "."
assert service["ports"] == ["127.0.0.1:3000:3000", "127.0.0.1:3001:3001"]
assert service["volumes"] == ["./config:/config", "${MODELS_DIR:-./models}:/models"]
assert service["stop_grace_period"] == "30s"
assert service["security_opt"] == ["no-new-privileges:true"]
assert "healthcheck" not in service
print("OK: Compose YAML parsed (PyYAML; not Docker Compose validation)")
PY
else
    printf 'SKIP: PyYAML unavailable; Compose checked with text assertions\n'
fi
for expected in 'ARG BASE_IMAGE=' 'FROM ${BASE_IMAGE}' 'ARG APP_URL' 'ARG APP_VERSION' '--appimage-extract' 'THREEMF_CONTAINER=1' 'de_DE.UTF-8' 'org.opencontainers.image.licenses=' 'STOPSIGNAL SIGTERM' 'HEALTHCHECK --interval=30s' 'init-nginx/dependencies.d/init-3mf' 'init-kasmvnc/dependencies.d/init-3mf'; do
    grep -Fq -- "$expected" Dockerfile
done
if grep -Fq '/usr/local/bin/3mf-katalog &' Dockerfile; then
    printf 'FAIL: one-shot autostart line in Dockerfile\n' >&2
    exit 1
fi
[[ $(cat root/etc/s6-overlay/s6-rc.d/svc-3mf/type) == longrun ]]
[[ $(cat root/etc/s6-overlay/s6-rc.d/init-3mf/type) == oneshot ]]
[[ -f root/etc/s6-overlay/s6-rc.d/user/contents.d/svc-3mf ]]
[[ -f root/etc/s6-overlay/s6-rc.d/user/contents.d/init-3mf ]]
[[ -f root/etc/s6-overlay/s6-rc.d/svc-3mf/dependencies.d/svc-kasmvnc ]]
[[ -f root/etc/s6-overlay/s6-rc.d/init-3mf/dependencies.d/init-adduser ]]
printf 'OK: Compose, Dockerfile and s6 structural checks (no Docker invocation)\n'
bash scripts/test-init.sh
timeout --kill-after=5s 15s bash scripts/test-supervisor.sh
printf 'All available static/local checks passed; image build and startup remain unverified.\n'
