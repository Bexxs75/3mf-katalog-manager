#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
auth="$root/root/usr/local/libexec/3mf-auth"
fs="$root/root/usr/local/libexec/3mf-filesystem"
[[ -x $auth && -x $fs ]] || fail 'Init helpers are missing or not executable'
owner="$(id -u):$(id -g)"
mkdir "$tmp/config" "$tmp/env"
run_auth() {
    env -u PASSWORD -u CUSTOM_USER -u ALLOW_NO_AUTH "$@" "$auth" "$tmp/config" "$tmp/env" "$owner"
}
run_auth > "$tmp/first"
password=$(cat "$tmp/config/.kasmvnc-password")
[[ ${#password} -ge 16 ]] || fail 'Generated password too short'
[[ $(stat -c %a "$tmp/config/.kasmvnc-password") == 600 ]] || fail 'Password permissions'
[[ $(stat -c %u:%g "$tmp/config/.kasmvnc-password") == "$owner" ]] || fail 'Password ownership'
grep -Fq "$password" "$tmp/first" || fail 'First-start password missing from log'
[[ $(cat "$tmp/env/PASSWORD") == "$password" ]] || fail 'Password not delivered to s6'
[[ $(cat "$tmp/env/CUSTOM_USER") == abc ]] || fail 'Default login user'
run_auth > "$tmp/second"
[[ $(cat "$tmp/config/.kasmvnc-password") == "$password" ]] || fail 'Password changed on restart'
! grep -Fq "$password" "$tmp/second" || fail 'Password repeated in log'
secret=$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')
run_auth "PASSWORD=$secret" CUSTOM_USER=example > "$tmp/explicit"
! grep -Fq "$secret" "$tmp/explicit" || fail 'Explicit password leaked'
[[ $(cat "$tmp/env/PASSWORD") == "$secret" ]] || fail 'Explicit password ignored'
[[ $(cat "$tmp/env/CUSTOM_USER") == example ]] || fail 'Custom user ignored'
[[ $(cat "$tmp/config/.kasmvnc-password") == "$password" ]] || fail 'Saved password overwritten'
run_auth "PASSWORD=$secret" CUSTOM_USER=example ALLOW_NO_AUTH=true > "$tmp/no-auth"
grep -q 'WARNUNG / WARNING' "$tmp/no-auth" || fail 'No-auth warning missing'
[[ ! -s $tmp/env/PASSWORD && ! -s $tmp/env/CUSTOM_USER ]] || fail 'No-auth retains authentication'
! grep -Fq "$secret" "$tmp/no-auth" || fail 'No-auth leaks supplied password'
run_auth CUSTOM_USER=other > "$tmp/custom"
[[ $(cat "$tmp/env/CUSTOM_USER") == other ]] || fail 'Custom user with generated password'
[[ $(cat "$tmp/env/PASSWORD") == "$password" ]] || fail 'Authentication not restored'
mv "$tmp/config/.kasmvnc-password" "$tmp/saved"
ln -s "$tmp/saved" "$tmp/config/.kasmvnc-password"
if run_auth > "$tmp/symlink" 2>&1; then fail 'Password symlink accepted'; fi
rm "$tmp/config/.kasmvnc-password"
run_auth "PASSWORD=$secret" > "$tmp/explicit-fresh"
[[ ! -e $tmp/config/.kasmvnc-password ]] || fail 'Explicit password generated a saved password'
! grep -Fq "$secret" "$tmp/explicit-fresh" || fail 'Fresh explicit password leaked'
run_auth ALLOW_NO_AUTH=true > "$tmp/no-auth-fresh"
[[ ! -e $tmp/config/.kasmvnc-password ]] || fail 'No-auth generated a password'
grep -q 'WARNUNG / WARNING' "$tmp/no-auth-fresh" || fail 'Fresh no-auth warning missing'
printf 'OK: password generation, reuse, precedence, no-auth, ownership and symlink rejection\n'
for kind in nfs nfs4 cifs smb2 smb3 fuse fuseblk fuse.sshfs 9p; do
    "$fs" "$tmp/config" "$owner" "$kind" > "$tmp/fs"
    [[ -f $tmp/config/.network-filesystem-warning && ! -s $tmp/config/.network-filesystem-warning ]] || fail "Missing marker for $kind"
    [[ $(stat -c %u:%g "$tmp/config/.network-filesystem-warning") == "$owner" ]] || fail 'Marker ownership'
    grep -q 'WARNUNG / WARNING' "$tmp/fs" || fail "Missing warning for $kind"
done
for kind in ext2/ext3 btrfs xfs overlayfs tmpfs; do
    touch "$tmp/config/.network-filesystem-warning"
    "$fs" "$tmp/config" "$owner" "$kind" > "$tmp/fs"
    [[ ! -e $tmp/config/.network-filesystem-warning ]] || fail "Stale marker for $kind"
done
printf 'OK: network/FUSE filesystem warnings and stale marker removal\n'
