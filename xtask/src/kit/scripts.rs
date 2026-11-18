//! The shell scripts the kit ships: the verifier and the offline server.

/// The kit's own verification script. It checks the whole manifest, then the
/// report bundle's nested manifest, refuses a manifest that does not describe
/// exactly the files present, and prints the measured size on disk.
pub(crate) fn render_verify_script() -> &'static str {
    r#"#!/bin/sh
# Verify every file on this drive against SHA256SUMS, then verify the report
# bundle against its own manifest. Portable across GNU coreutils and macOS.
# Prints a line only for a problem; exits non-zero when anything is wrong.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$root"

if [ ! -f SHA256SUMS ]; then
    echo "verify: SHA256SUMS is missing from $root" >&2
    exit 2
fi

failures=0

check_manifest() {
    manifest=$1
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum --check --strict --quiet "$manifest"
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 --check "$manifest" >/dev/null
    else
        echo "verify: neither sha256sum nor shasum is installed" >&2
        return 2
    fi
}

if ! check_manifest SHA256SUMS; then
    echo "verify: the kit manifest does not match the files on this drive" >&2
    failures=$((failures + 1))
fi

tmp=${TMPDIR:-/tmp}
work=$(mktemp -d "$tmp/climb-verify.XXXXXX") || {
    echo "verify: cannot create a temporary directory under $tmp" >&2
    exit 2
}
trap 'rm -rf "$work"' EXIT INT TERM HUP

sed -n 's/^[0-9a-f]\{64\}  //p' SHA256SUMS | LC_ALL=C sort >"$work/listed"
find . -type f \
    ! -name '.DS_Store' \
    ! -name '._*' \
    ! -path './.Spotlight-V100/*' \
    ! -path './.Trashes/*' \
    ! -path './.fseventsd/*' \
    ! -path './SHA256SUMS' |
    sed 's|^\./||' | LC_ALL=C sort >"$work/actual"

if ! cmp -s "$work/listed" "$work/actual"; then
    echo "verify: the manifest and the files on this drive disagree" >&2
    diff -u "$work/listed" "$work/actual" |
        sed -n 's/^\([+-][^+-].*\)/  \1/p' >&2 || true
    failures=$((failures + 1))
fi

if [ -e bin/climb-tui ] && [ ! -x bin/climb-tui ]; then
    echo "verify: note: bin/climb-tui has no executable bit here (a FAT or exFAT" >&2
    echo "  drive cannot carry one). Copy it to a local directory and chmod +x it." >&2
fi

if [ -f report/SHA256SUMS ]; then
    if ! ( cd report && check_manifest SHA256SUMS ); then
        echo "verify: the report bundle does not match report/SHA256SUMS" >&2
        failures=$((failures + 1))
    fi
else
    echo "verify: report/SHA256SUMS is missing" >&2
    failures=$((failures + 1))
fi

if command -v du >/dev/null 2>&1; then
    size=$(du -sk . 2>/dev/null | awk '{print $1}')
    echo "verify: whole kit is $size KiB on disk"
fi

if [ "$failures" -ne 0 ]; then
    echo "verify: $failures problem(s)" >&2
    exit 1
fi

echo "verify: every file matches SHA256SUMS (report bundle included)"
"#
}

/// The kit's offline site server. Loopback-only, with the deployment's 404
/// behaviour, no directory listings and no network access of its own.
pub(crate) fn render_serve_script() -> &'static str {
    r#"#!/bin/sh
# Serve the built site in this kit on the loopback address. No network service
# is contacted; no directory listing is exposed; a missing page serves the
# site's own 404.html when the build ships one.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
docroot="$root/site/dist"
port=${1:-8080}

if [ ! -d "$docroot" ]; then
    echo "serve: $docroot is missing; verify the kit with ./verify.sh" >&2
    exit 2
fi

if ! command -v python3 >/dev/null 2>&1; then
    echo "serve: python3 is required; open report/report.html instead" >&2
    exit 2
fi

exec python3 - "$docroot" "$port" <<'PY'
import http.server
import os
import sys

root = os.path.realpath(sys.argv[1])
try:
    port = int(sys.argv[2])
except ValueError:
    sys.exit("serve: the port must be a number")


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=root, **kwargs)

    def list_directory(self, path):
        return None

    def end_headers(self):
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def send_error(self, code, message=None, explain=None):
        if code == 404:
            page = os.path.join(root, "404.html")
            if os.path.isfile(page):
                try:
                    with open(page, "rb") as handle:
                        body = handle.read()
                except OSError:
                    body = None
                if body is not None:
                    self.send_response(404)
                    self.send_header("Content-Type", "text/html; charset=utf-8")
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    if self.command != "HEAD":
                        self.wfile.write(body)
                    return
        super().send_error(code, message, explain)


class Server(http.server.ThreadingHTTPServer):
    daemon_threads = True


try:
    server = Server(("127.0.0.1", port), Handler)
except OSError as error:
    sys.exit("serve: cannot bind 127.0.0.1:%d (%s)" % (port, error))

bound = server.server_address[1]
print("serve: http://127.0.0.1:%d/  (site: %s)" % (bound, root), flush=True)
print("serve: press Ctrl-C to stop", flush=True)
try:
    server.serve_forever()
except KeyboardInterrupt:
    pass
finally:
    server.server_close()
PY
"#
}
