#!/usr/bin/env sh
# CellScript installer — curl -fsSL <URL> | sh
set -eu
if (set -o pipefail) 2>/dev/null; then
    set -o pipefail
fi

REPO="tsukifune-kosei/CellScript"
BINARY="cellc"
INSTALL_DIR="${CELLSCRIPT_HOME:-$HOME/.cellscript}/bin"

# ---------------------------------------------------------------------------
# Mirror sources — try in order until one succeeds
#
# Set CELLSCRIPT_MIRROR to override, e.g.:
#   CELLSCRIPT_MIRROR=ghgo     — use ghgo.xyz GitHub proxy (China-friendly)
#   CELLSCRIPT_MIRROR=ghproxy  — use gh-proxy.com (China-friendly)
#   CELLSCRIPT_MIRROR=ghfast   — use ghfast.top (China-friendly)
#   CELLSCRIPT_MIRROR=direct   — force github.com only, no mirrors
# ---------------------------------------------------------------------------

# github.com release download rewrite for each mirror
# Each function accepts (REPO, VERSION, FILENAME) and prints the full URL.
_github_url()  { printf 'https://github.com/%s/releases/download/v%s/%s' "$1" "$2" "$3"; }
_ghgo_url()    { printf 'https://ghgo.xyz/https://github.com/%s/releases/download/v%s/%s' "$1" "$2" "$3"; }
_ghproxy_url() { printf 'https://gh-proxy.com/https://github.com/%s/releases/download/v%s/%s' "$1" "$2" "$3"; }
_ghfast_url()  { printf 'https://ghfast.top/https://github.com/%s/releases/download/v%s/%s' "$1" "$2" "$3"; }

MIRROR_FUNCS="_github_url _ghgo_url _ghproxy_url _ghfast_url"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

info()  { printf '[cellscript] %s\n' "$1"; }
warn()  { printf '[cellscript] WARNING: %s\n' "$1" >&2; }
die()   { printf '[cellscript] ERROR: %s\n' "$1" >&2; exit 1; }

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        die "required command not found: $1"
    fi
}

# Try curl with multiple URL sources, return on first success.
curl_any() {
    # Usage: curl_any <arg1> <arg2> ... -- <url1> <url2> ...
    _curl_args=""
    _urls=""
    _after_separator=0
    for _arg in "$@"; do
        if [ "$_arg" = "--" ]; then
            _after_separator=1
            continue
        fi
        if [ "$_after_separator" -eq 0 ]; then
            _curl_args="$_curl_args $_arg"
        else
            _urls="$_urls $_arg"
        fi
    done

    for _url in $_urls; do
        if curl $_curl_args "$_url"; then
            return 0
        fi
        warn "source unreachable: $_url"
    done
    return 1
}

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------

detect_platform() {
    _OS="$(uname -s)"
    _ARCH="$(uname -m)"

    case "$_OS" in
        Darwin) _PLATFORM="apple-darwin" ;;
        Linux)  _PLATFORM="unknown-linux-musl" ;;
        MINGW*|MSYS*|CYGWIN*) _PLATFORM="pc-windows-msvc"; _EXT=".exe" ;;
        *)      die "unsupported operating system: $_OS" ;;
    esac

    case "$_ARCH" in
        x86_64|amd64)  _ARCH="x86_64" ;;
        aarch64|arm64) _ARCH="aarch64" ;;
        *)             die "unsupported architecture: $_ARCH" ;;
    esac

    TARGET="${_ARCH}-${_PLATFORM}"
}

# ---------------------------------------------------------------------------
# Version resolution
# ---------------------------------------------------------------------------

get_latest_version() {
    # Accept explicit version from environment
    if [ -n "${CELLSCRIPT_VERSION:-}" ]; then
        VERSION="$CELLSCRIPT_VERSION"
        info "using requested version: $VERSION"
        return 0
    fi

    need_cmd curl

    # Try multiple version sources — api.github.com is often blocked in China

    # Source 1: GitHub API (global)
    _TAG="$(curl -fsSL --max-time 10 "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')"
    if [ -n "$_TAG" ]; then
        VERSION="$_TAG"
        info "latest version: $VERSION"
        return 0
    fi

    # Source 2: GitHub API via ghgo proxy (China-friendly)
    _TAG="$(curl -fsSL --max-time 10 "https://ghgo.xyz/https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')"
    if [ -n "$_TAG" ]; then
        VERSION="$_TAG"
        info "latest version: $VERSION (via ghgo)"
        return 0
    fi

    # Source 3: GitHub API via ghfast proxy (China-friendly)
    _TAG="$(curl -fsSL --max-time 10 "https://ghfast.top/https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')"
    if [ -n "$_TAG" ]; then
        VERSION="$_TAG"
        info "latest version: $VERSION (via ghfast)"
        return 0
    fi

    # Source 4: GitHub tags API fallback
    _TAG="$(curl -fsSL --max-time 10 "https://api.github.com/repos/${REPO}/tags" 2>/dev/null \
            | grep '"name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')"
    if [ -n "$_TAG" ]; then
        VERSION="$_TAG"
        info "latest version: $VERSION"
        return 0
    fi

    die "unable to determine the latest CellScript version. Set CELLSCRIPT_VERSION manually."
}

# ---------------------------------------------------------------------------
# Download and install
# ---------------------------------------------------------------------------

# Resolve the list of mirror functions to try for binary downloads.
_resolve_mirrors() {
    case "${CELLSCRIPT_MIRROR:-auto}" in
        direct)   MIRRORS="_github_url" ;;
        ghgo)     MIRRORS="_ghgo_url _github_url" ;;
        ghproxy)  MIRRORS="_ghproxy_url _github_url" ;;
        ghfast)   MIRRORS="_ghfast_url _github_url" ;;
        auto|"") MIRRORS="$MIRROR_FUNCS" ;;
        *)        die "unknown CELLSCRIPT_MIRROR value: $CELLSCRIPT_MIRROR (use: auto, direct, ghgo, ghproxy, ghfast)" ;;
    esac
}

download_and_install() {
    _ARCHIVE="cellscript-${VERSION}-${TARGET}.tar.gz"

    _TMPDIR="$(mktemp -d)"
    _CLEANUP=1
    trap '_cleanup' EXIT

    # Build URL list from mirrors
    _resolve_mirrors
    _URLS=""
    for _mf in $MIRRORS; do
        _URLS="$_URLS $($_mf "$REPO" "$VERSION" "$_ARCHIVE")"
    done

    info "downloading ${_ARCHIVE} ..."
    curl_any -fsSL --max-time 120 -o "$_TMPDIR/$_ARCHIVE" -- $_URLS \
        || die "download failed from all mirrors"

    # Optional SHA256 verification — try same mirrors for SHA256SUMS
    _SHA_URLS=""
    for _mf in $MIRRORS; do
        _SHA_URLS="$_SHA_URLS $($_mf "$REPO" "$VERSION" "SHA256SUMS")"
    done
    if curl_any -fsSL --max-time 15 -o "$_TMPDIR/SHA256SUMS" -- $_SHA_URLS 2>/dev/null; then
        info "verifying SHA256 checksum ..."
        cd "$_TMPDIR"
        if command -v sha256sum >/dev/null 2>&1; then
            printf '%s' "$(grep "$_ARCHIVE" SHA256SUMS)" | sha256sum -c - || die "SHA256 checksum mismatch"
        elif command -v shasum >/dev/null 2>&1; then
            printf '%s' "$(grep "$_ARCHIVE" SHA256SUMS)" | shasum -a 256 -c - || die "SHA256 checksum mismatch"
        else
            warn "sha256sum/shasum not found; skipping checksum verification"
        fi
        cd -
    else
        warn "SHA256SUMS not found for v${VERSION}; skipping checksum verification"
    fi

    info "installing to ${INSTALL_DIR} ..."
    mkdir -p "$INSTALL_DIR"
    tar xzf "$_TMPDIR/$_ARCHIVE" -C "$_TMPDIR" "${BINARY}${_EXT:-}"
    mv "$_TMPDIR/${BINARY}${_EXT:-}" "$INSTALL_DIR/${BINARY}${_EXT:-}"
    chmod +x "$INSTALL_DIR/${BINARY}${_EXT:-}"
}

_cleanup() {
    if [ -n "${_TMPDIR:-}" ] && [ -d "$_TMPDIR" ] && [ "$_CLEANUP" -eq 1 ]; then
        rm -rf "$_TMPDIR"
    fi
}

# ---------------------------------------------------------------------------
# PATH configuration
# ---------------------------------------------------------------------------

ensure_path() {
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) info "PATH already contains ${INSTALL_DIR}"; return 0 ;;
    esac

    _RC_FILE=""
    case "${SHELL:-}" in
        */fish) _RC_FILE="${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish" ;;
        */zsh)  _RC_FILE="$HOME/.zshrc" ;;
        */bash) _RC_FILE="$HOME/.bashrc" ;;
        */sh)   _RC_FILE="$HOME/.profile" ;;
    esac

    if [ -z "$_RC_FILE" ]; then
        warn "could not detect shell config file. Add ${INSTALL_DIR} to your PATH manually."
        return 0
    fi

    if ! grep -q "$INSTALL_DIR" "$_RC_FILE" 2>/dev/null; then
        mkdir -p "$(dirname "$_RC_FILE")"

        case "${SHELL:-}" in
            */fish)
                printf 'set -gx PATH %s $PATH\n' "$INSTALL_DIR" >> "$_RC_FILE"
                ;;
            *)
                printf 'export PATH="%s:$PATH"\n' "$INSTALL_DIR" >> "$_RC_FILE"
                ;;
        esac
        info "added ${INSTALL_DIR} to PATH in ${_RC_FILE}"
    fi

    export PATH="${INSTALL_DIR}:${PATH}"
}

# ---------------------------------------------------------------------------
# Dry-run support
# ---------------------------------------------------------------------------

dry_run() {
    info "=== DRY RUN ==="
    info "platform:  ${TARGET}"
    info "version:   ${VERSION}"
    info "binary:    ${BINARY}"
    info "install:   ${INSTALL_DIR}/${BINARY}${_EXT:-}"
    info "mirrors:   ${CELLSCRIPT_MIRROR:-auto}"
    _resolve_mirrors
    for _mf in $MIRRORS; do
        info "source:    $($_mf "$REPO" "$VERSION" "cellscript-${VERSION}-${TARGET}.tar.gz")"
    done
    info "===============  No files were modified."
    exit 0
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    need_cmd curl
    need_cmd tar
    need_cmd uname

    detect_platform
    get_latest_version

    if [ "${CELLSCRIPT_DRY_RUN:-0}" = "1" ]; then
        dry_run
    fi

    download_and_install
    ensure_path

    # Verify installation
    if command -v "$BINARY" >/dev/null 2>&1; then
        _VER="$("$BINARY" --version 2>/dev/null || true)"
        info "installation complete! ${_VER}"
    else
        info "installation complete!"
        info "restart your shell or run: export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi
}

main "$@"
