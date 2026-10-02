#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary_path=${CODEX_SIDEBAR_BINARY:-"$repo_root/codex-rs/target/release/codex"}
codex_home=${CODEX_HOME:-"$HOME/.codex"}
base_package=${CODEX_SIDEBAR_BASE_PACKAGE:-"$codex_home/packages/standalone/current"}
install_path=${CODEX_SIDEBAR_DESTINATION:-"$HOME/.local/bin/codex-sidebar"}
install_root=${CODEX_SIDEBAR_INSTALL_ROOT:-"$HOME/.local/lib/codex-sidebar"}
releases_path="$install_root/releases"
current_path="$install_root/current"
install_marker="$install_root/.managed-by-session-switcher"
launcher_marker="# codex-sidebar managed launcher"

if [ ! -x "$binary_path" ]; then
    printf 'Build the session switcher first; missing executable: %s\n' "$binary_path" >&2
    exit 1
fi
if [ ! -f "$base_package/codex-package.json" ] \
    || [ ! -x "$base_package/bin/codex-code-mode-host" ] \
    || [ ! -x "$base_package/codex-path/rg" ]; then
    printf 'A complete installed Codex package is required at %s\n' "$base_package" >&2
    exit 1
fi

manifest_version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' \
    "$base_package/codex-package.json")
binary_version=$($binary_path --version | awk '{print $NF}')
if [ "$manifest_version" != "$binary_version" ]; then
    printf 'Codex package version (%s) does not match the switcher build (%s)\n' \
        "$manifest_version" "$binary_version" >&2
    exit 1
fi

binary_hash=$(shasum -a 256 "$binary_path" | awk '{print $1}')
binary_hash_short=$(printf '%s' "$binary_hash" | cut -c1-12)
release_name="$binary_version-session-switcher-$binary_hash_short"
release_path="$releases_path/$release_name"

if [ -e "$install_root" ] && [ ! -f "$install_marker" ]; then
    printf 'Refusing to use an unmanaged install directory: %s\n' "$install_root" >&2
    exit 1
fi
if [ -e "$install_path" ] && ! grep -Fq "$launcher_marker" "$install_path"; then
    if ! cmp -s "$install_path" "$binary_path"; then
        printf 'Refusing to replace an existing command: %s\n' "$install_path" >&2
        exit 1
    fi
fi
if [ -e "$current_path" ] && [ ! -L "$current_path" ]; then
    printf 'Refusing to replace a non-symlink package path: %s\n' "$current_path" >&2
    exit 1
fi

mkdir -p "$releases_path" "$(dirname -- "$install_path")"
staging_path=$(mktemp -d "$releases_path/.staging.XXXXXX")
launcher_temporary="${install_path}.tmp.$$"
cleanup() {
    rm -rf "$staging_path"
    rm -f "$launcher_temporary"
}
trap cleanup EXIT HUP INT TERM

if [ -d "$release_path" ]; then
    if ! cmp -s "$binary_path" "$release_path/bin/codex"; then
        printf 'Release path exists with different contents: %s\n' "$release_path" >&2
        exit 1
    fi
    rm -rf "$staging_path"
else
    ditto "$base_package" "$staging_path"
    cp "$binary_path" "$staging_path/bin/codex.tmp"
    chmod 755 "$staging_path/bin/codex.tmp"
    mv "$staging_path/bin/codex.tmp" "$staging_path/bin/codex"
    mv "$staging_path" "$release_path"
fi

printf '%s\n' 'codex-sidebar' > "$install_marker"
ln -sfn "releases/$release_name" "$current_path"

cat > "$launcher_temporary" <<'EOF'
#!/bin/sh
# codex-sidebar managed launcher
set -eu
package_root=${CODEX_SIDEBAR_PACKAGE_ROOT:-"$HOME/.local/lib/codex-sidebar/current"}
exec "$package_root/bin/codex" "$@"
EOF
chmod 755 "$launcher_temporary"
mv -f "$launcher_temporary" "$install_path"

trap - EXIT HUP INT TERM
cleanup
printf 'Installed %s\n' "$install_path"
printf 'Package: %s\n' "$release_path"
