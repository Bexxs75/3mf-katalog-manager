#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
tmp=$(mktemp -d)
pid=
cleanup() {
    if [[ -n $pid ]]; then kill -TERM "$pid" 2>/dev/null || :; wait "$pid" || :; fi
    rm -rf -- "$tmp"
}
trap cleanup EXIT
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
supervisor="$root/root/usr/local/libexec/3mf-supervise"
[[ -x $supervisor ]] || fail 'Supervisor missing or not executable'
mkdir "$tmp/state"
cat > "$tmp/app" <<'APP'
#!/usr/bin/env bash
trap 'echo terminated; exit 0' TERM
echo stdout-ready
echo stderr-ready >&2
while :; do sleep 1 & wait $!; done
APP
chmod +x "$tmp/app"
"$supervisor" "$tmp/state" "$tmp/app" > "$tmp/log" 2>&1 &
pid=$!
for ((i=0; i<50; i++)); do
    if grep -q stdout-ready "$tmp/log" && [[ -s $tmp/state/app.pid ]]; then break; fi
    sleep 0.1
done
[[ -s $tmp/state/app.pid ]] || fail 'No app PID'
app_pid=$(cat "$tmp/state/app.pid")
grep -q stderr-ready "$tmp/log" || fail 'stderr not inherited'
kill -TERM "$pid"
wait "$pid"
pid=
grep -q terminated "$tmp/log" || fail 'SIGTERM not forwarded'
! kill -0 "$app_pid" 2>/dev/null || fail 'App survived supervisor shutdown'
[[ ! -e $tmp/state/app.pid ]] || fail 'Stale PID after shutdown'
"$supervisor" "$tmp/state" /bin/false > "$tmp/crash" 2>&1 &
pid=$!
for ((i=0; i<60; i++)); do
    if grep -q '4 s' "$tmp/crash"; then break; fi
    sleep 0.1
done
grep -q '2 s' "$tmp/crash" || fail 'Initial restart delay missing'
grep -q '4 s' "$tmp/crash" || fail 'Crash backoff missing'
[[ ! -e $tmp/state/app.pid ]] || fail 'Dead app reported running'
kill -TERM "$pid"
wait "$pid"
pid=
printf 'OK: supervisor stdout/stderr, SIGTERM, PID cleanup and crash backoff\n'
