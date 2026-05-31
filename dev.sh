#!/bin/bash
set -e

export RUST_BACKTRACE=1

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PBF_DIR="$SCRIPT_DIR/map-data/pbf"
INPUT_DIR="$SCRIPT_DIR/map-data/input"
OUTPUT_DIR="$SCRIPT_DIR/map-data/output"
ROUTE_DIR="$SCRIPT_DIR/map-data/routes"
PROGRESS_DIR="$SCRIPT_DIR/map-data/progress"
RULE_EXAMPLES_DIR="$SCRIPT_DIR/rule-examples"
OSM_USER_AGENT="ridi-router-dev-script/1.0 (local development helper)"

# PBF URL mappings
declare -A PBF_URLS
PBF_URLS[latvia]="https://download.geofabrik.de/europe/latvia-latest.osm.pbf"
PBF_URLS[estonia]="https://download.geofabrik.de/europe/estonia-latest.osm.pbf"
PBF_URLS[lithuania]="https://download.geofabrik.de/europe/lithuania-latest.osm.pbf"
PBF_URLS[montenegro]="https://download.geofabrik.de/europe/montenegro-latest.osm.pbf"

list_rule_presets() {
    local -a presets=()
    local file

    shopt -s nullglob
    for file in "$RULE_EXAMPLES_DIR"/rules-*.json; do
        local preset
        preset="$(basename "$file")"
        preset="${preset#rules-}"
        preset="${preset%.json}"
        presets+=("$preset")
    done
    shopt -u nullglob

    if [[ ${#presets[@]} -eq 0 ]]; then
        echo "none"
        return 0
    fi

    printf '%s\n' "${presets[*]}"
}

cmd_help() {
    echo "Usage: ./dev.sh <command> [args]"
    echo ""
    echo "Commands:"
    echo "  pbf <country>                       Download PBF file for a country"
    echo "  generate-tiles <countries>          Generate tiles for comma-separated countries"
    echo "  route [-p] <start> <finish> [preset]  Generate a GPX route using OSM place lookup"
    echo "  build                               Build the project"
    echo "  run                                 Run the CLI"
    echo "  rmdf-view                           Run the RMDF debug viewer"
    echo "  test                                Run tests"
    echo "  app-build                           Build Android release APK"
    echo "  app-build-debug                     Build Android debug APK"
    echo "  app-run-debug                       Build, install, launch, and stream logs via adb"
    echo "  app-logcat                          Stream Android logs for app debugging"
    echo "  rust-router android                 Build/copy Rust Android libraries for all ABIs"
    echo "  rust-router ios                     Build/copy Rust iOS xcframework"
    echo "  rust-router bindings                Generate/copy UniFFI Kotlin and Swift bindings"
    echo "  rust-router build                   Generate bindings and build all native artifacts"
    echo "  mobile android device               Launch Expo Android app on device"
    echo "  mobile android simulator            Launch Expo Android app on emulator"
    echo "  mobile ios device                   Launch Expo iOS app on device"
    echo "  mobile ios simulator                Launch Expo iOS app on simulator"
    echo ""
    echo "Examples:"
    echo "  ./dev.sh generate-tiles latvia"
    echo "  ./dev.sh route riga,latvia cesis,latvia"
    echo "  ./dev.sh route riga,latvia cesis,latvia prefer-unpaved"
    echo "  ./dev.sh route -p riga,latvia cesis,latvia prefer-unpaved"
    echo "  ./dev.sh app-run-debug"
    echo "  ./dev.sh app-logcat"
    echo ""
    echo "Route preset mapping: <preset> -> $RULE_EXAMPLES_DIR/rules-<preset>.json"
    echo "If no preset is provided, dev.sh uses the default preset."
    echo "Available presets: $(list_rule_presets)"
    echo ""
    echo "Profiling: RIDI_FEATURES=perf expands to hotpath,hotpath-alloc,hotpath-mcp."
    echo "./dev.sh run also switches to --release when perf is requested."
    echo ""
    echo "Available countries: ${!PBF_URLS[*]}"
}

download_pbf() {
    local country="$1"
    local url="${PBF_URLS[$country]}"

    if [[ -z "$url" ]]; then
        echo "Error: Unknown country '$country'. Available: ${!PBF_URLS[*]}" >&2
        exit 1
    fi

    mkdir -p "$PBF_DIR"
    local filename="$country-latest.osm.pbf"
    local filepath="$PBF_DIR/$filename"

    if [[ -f "$filepath" ]]; then
        echo "PBF already exists: $filepath"
        return 0
    fi

    echo "Downloading $url..."
    curl -L -o "$filepath" "$url"
    echo "Saved to $filepath"
}

run_cli_feature_set() {
    local raw_features="$1"
    local -a requested_features=()
    local -a resolved_features=()
    local feature
    local perf_feature
    local -A seen_features=()

    if [[ -z "$raw_features" ]]; then
        return 0
    fi

    IFS=',' read -ra requested_features <<< "$raw_features"
    for feature in "${requested_features[@]}"; do
        feature=$(echo "$feature" | xargs)
        if [[ -z "$feature" ]]; then
            continue
        fi

        if [[ "$feature" == "perf" ]]; then
            for perf_feature in hotpath hotpath-alloc hotpath-mcp; do
                if [[ -z "${seen_features[$perf_feature]:-}" ]]; then
                    resolved_features+=("$perf_feature")
                    seen_features["$perf_feature"]=1
                fi
            done
            continue
        fi

        if [[ -z "${seen_features[$feature]:-}" ]]; then
            resolved_features+=("$feature")
            seen_features["$feature"]=1
        fi
    done

    if [[ ${#resolved_features[@]} -gt 0 ]]; then
        local IFS=,
        printf '%s\n' "${resolved_features[*]}"
    fi
}

merge_cli_feature_sets() {
    local base_features="$1"
    local env_features="$2"
    local merged_features=""
    local expanded_features

    expanded_features="$(run_cli_feature_set "$base_features")"
    if [[ -n "$expanded_features" ]]; then
        merged_features="$expanded_features"
    fi

    expanded_features="$(run_cli_feature_set "$env_features")"
    if [[ -n "$expanded_features" ]]; then
        if [[ -n "$merged_features" ]]; then
            merged_features="$(run_cli_feature_set "$merged_features,$expanded_features")"
        else
            merged_features="$expanded_features"
        fi
    fi

    printf '%s\n' "$merged_features"
}

cli_perf_requested() {
    local -a requested_features=()
    local feature
    local raw_features="${RIDI_FEATURES:-}"

    if [[ -z "$raw_features" ]]; then
        return 1
    fi

    IFS=',' read -ra requested_features <<< "$raw_features"
    for feature in "${requested_features[@]}"; do
        feature=$(echo "$feature" | xargs)
        if [[ "$feature" == "perf" ]]; then
            return 0
        fi
    done

    return 1
}

run_cli() {
    local profile="$1"
    local extra_features="$2"
    shift 2

    local features
    local -a cargo_args

    features="$(merge_cli_feature_sets "$extra_features" "${RIDI_FEATURES:-}")"
    cargo_args=(cargo run -p ridi-router-cli)

    if [[ "$profile" == "release" ]]; then
        cargo_args+=(--release)
    fi

    if [[ -n "$features" ]]; then
        cargo_args+=(--features "$features")
    fi

    cargo_args+=(-- "$@")
    "${cargo_args[@]}"
}

run_cli_release() {
    run_cli release "" "$@"
}

run_cli_release_with_features() {
    local features="$1"
    shift
    run_cli release "$features" "$@"
}

lookup_osm_coords() {
    local query="$1"
    local encoded_query
    local response
    local parsed

    encoded_query="$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1]))' "$query")"
    response="$(curl -fsSL -A "$OSM_USER_AGENT" "https://nominatim.openstreetmap.org/search?format=jsonv2&limit=1&q=$encoded_query")"

    parsed="$(python3 -c 'import json, sys
places = json.load(sys.stdin)
if not places:
    sys.exit(1)
place = places[0]
print(f"{place['"'"'lat'"'"']},{place['"'"'lon'"'"']}\t{place.get('"'"'display_name'"'"', '"'"''"'"')}")
' <<< "$response")" || {
        echo "Error: OpenStreetMap lookup returned no match for '$query'" >&2
        exit 1
    }

    printf '%s\n' "$parsed"
}

resolve_route_coords() {
    local input="$1"

    if [[ "$input" =~ ^-?[0-9]+([.][0-9]+)?,-?[0-9]+([.][0-9]+)?$ ]]; then
        printf '%s\t%s\n' "$input" "$input"
        return 0
    fi

    lookup_osm_coords "$input"
}

cmd_generate_tiles() {
    local countries="$1"

    if [[ -z "$countries" ]]; then
        echo "Error: No countries specified. Usage: ./dev.sh generate-tiles latvia,estonia" >&2
        exit 1
    fi

    IFS=',' read -ra country_array <<< "$countries"

    # Validate all countries first
    for country in "${country_array[@]}"; do
        country=$(echo "$country" | xargs)  # trim whitespace
        if [[ -z "${PBF_URLS[$country]}" ]]; then
            echo "Error: Unknown country '$country'. Available: ${!PBF_URLS[*]}" >&2
            exit 1
        fi
    done

    # Download PBFs if needed
    for country in "${country_array[@]}"; do
        country=$(echo "$country" | xargs)
        download_pbf "$country"
    done

    # Clean output and input directories
    echo "Cleaning $OUTPUT_DIR..."
    rm -rf "$OUTPUT_DIR"
    mkdir -p "$OUTPUT_DIR"

    echo "Cleaning $INPUT_DIR..."
    rm -rf "$INPUT_DIR"
    mkdir -p "$INPUT_DIR"

    # Copy PBFs to input
    for country in "${country_array[@]}"; do
        country=$(echo "$country" | xargs)
        local src="$PBF_DIR/$country-latest.osm.pbf"
        local dest="$INPUT_DIR/$country-latest.osm.pbf"
        echo "Copying $src -> $dest"
        cp "$src" "$dest"
    done

    # Run tile generation
    echo "Generating tiles..."
    run_cli_release_with_features "debug-polygons" generate-tiles --input-dir "$INPUT_DIR" --output "$OUTPUT_DIR" --tile-size-deg 0.1
}

cmd_route() {
    local progress_enabled=false
    local -a positional_args=()
    local start_input
    local finish_input
    local preset
    local rule_file=""
    local start_result
    local finish_result
    local start_coords
    local finish_coords
    local start_name
    local finish_name
    local -a route_args

    while [[ $# -gt 0 ]]; do
        case "$1" in
            -p|--progress)
                progress_enabled=true
                shift
                ;;
            --)
                shift
                positional_args+=("$@")
                break
                ;;
            -*)
                echo "Error: Unknown route option '$1'. Usage: ./dev.sh route [-p] riga,latvia cesis,latvia [preset]" >&2
                exit 1
                ;;
            *)
                positional_args+=("$1")
                shift
                ;;
        esac
    done

    start_input="${positional_args[0]:-}"
    finish_input="${positional_args[1]:-}"
    preset="${positional_args[2]:-}"

    if [[ -z "$start_input" || -z "$finish_input" ]]; then
        echo "Error: Missing route locations. Usage: ./dev.sh route [-p] riga,latvia cesis,latvia [preset]" >&2
        exit 1
    fi

    if [[ ${#positional_args[@]} -gt 3 ]]; then
        echo "Error: Too many arguments. Usage: ./dev.sh route [-p] riga,latvia cesis,latvia [preset]" >&2
        exit 1
    fi

    if [[ ! -f "$OUTPUT_DIR/manifest.json" ]]; then
        echo "Error: Tiles not found in $OUTPUT_DIR. Run ./dev.sh generate-tiles <countries> first." >&2
        exit 1
    fi

    if [[ -n "$preset" ]]; then
        rule_file="$RULE_EXAMPLES_DIR/rules-$preset.json"
        if [[ ! -f "$rule_file" ]]; then
            echo "Error: Unknown route preset '$preset'." >&2
            echo "Expected preset file: $rule_file" >&2
            echo "Available presets: $(list_rule_presets)" >&2
            exit 1
        fi
        echo "Using rule preset: $rule_file"
    else
        rule_file="$RULE_EXAMPLES_DIR/rules-default.json"
        echo "Using default rule preset: $rule_file"
    fi

    echo "Resolving start via OpenStreetMap: $start_input"
    start_result="$(resolve_route_coords "$start_input")"
    echo "Resolving finish via OpenStreetMap: $finish_input"
    finish_result="$(resolve_route_coords "$finish_input")"

    start_coords="${start_result%%$'\t'*}"
    finish_coords="${finish_result%%$'\t'*}"
    start_name="${start_result#*$'\t'}"
    finish_name="${finish_result#*$'\t'}"

    echo "Start:  ${start_name:-$start_input} -> $start_coords"
    echo "Finish: ${finish_name:-$finish_input} -> $finish_coords"

    echo "Cleaning $ROUTE_DIR..."
    rm -rf "$ROUTE_DIR"
    if [[ "$progress_enabled" == true ]]; then
        echo "Cleaning $PROGRESS_DIR..."
        rm -rf "$PROGRESS_DIR"
    fi

    route_args=(
        generate-route
        --tiles "$OUTPUT_DIR"
        --output-dir "$ROUTE_DIR"
        --format gpx
    )

    if [[ "$progress_enabled" == true ]]; then
        route_args+=(--progress-dir "$PROGRESS_DIR")
    fi

    if [[ -n "$rule_file" ]]; then
        route_args+=(--rule-file "$rule_file")
    fi

    route_args+=(
        start-finish
        --start "$start_coords"
        --finish "$finish_coords"
    )

    echo "Generating route..."
    run_cli_release "${route_args[@]}"
    echo "Route files written to $ROUTE_DIR"
    if [[ "$progress_enabled" == true ]]; then
        echo "Progress files written to $PROGRESS_DIR"
    fi
}

cmd_build() {
    cargo build
}

cmd_run() {
    if cli_perf_requested; then
        run_cli release "" "$@"
    else
        run_cli debug "" "$@"
    fi
}

cmd_test() {
    cargo test
}

cmd_rmdf_view() {
    run_cli_release_with_features "rmdf-viewer" rmdf-viewer --input-dir "$OUTPUT_DIR"
}

cmd_app_build() {
    local profile="${1:-release}"
    local apk_profile_dir

    if [[ "$profile" != "release" && "$profile" != "debug" ]]; then
        echo "Error: unknown Android app build profile '$profile'" >&2
        exit 1
    fi

    ANDROID_APP_BUILD_DIR="$SCRIPT_DIR/target/android-ridi-app"
    ANDROID_CARGO_REGISTRY_DIR="$SCRIPT_DIR/target/android-cargo-registry"
    ANDROID_CARGO_GIT_DIR="$SCRIPT_DIR/target/android-cargo-git"
    ANDROID_RUSTUP_HOME="$SCRIPT_DIR/target/android-rustup-home"
    ANDROID_DOT_ANDROID_DIR="$SCRIPT_DIR/target/android-dot-android"
    ANDROID_RUST_TOOLCHAIN="${ANDROID_RUST_TOOLCHAIN:-stable}"
    apk_profile_dir="$profile"

    mkdir -p \
        "$ANDROID_APP_BUILD_DIR" \
        "$ANDROID_CARGO_REGISTRY_DIR" \
        "$ANDROID_CARGO_GIT_DIR" \
        "$ANDROID_RUSTUP_HOME" \
        "$ANDROID_DOT_ANDROID_DIR"

    rm -rf "$ANDROID_APP_BUILD_DIR/src" "$ANDROID_APP_BUILD_DIR/assets"
    cp "$SCRIPT_DIR/crates/ridi-app/Cargo.toml" "$ANDROID_APP_BUILD_DIR/"
    cp "$SCRIPT_DIR/crates/ridi-app/map-rendering.ron" "$ANDROID_APP_BUILD_DIR/"
    cp -R "$SCRIPT_DIR/crates/ridi-app/src" "$ANDROID_APP_BUILD_DIR/"
    cp -R "$SCRIPT_DIR/crates/ridi-app/assets" "$ANDROID_APP_BUILD_DIR/"

    docker run --rm \
       -e ANDROID_RUST_TOOLCHAIN="$ANDROID_RUST_TOOLCHAIN" \
       -e ANDROID_APP_BUILD_PROFILE="$profile" \
       -e CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git \
       -v "$ANDROID_APP_BUILD_DIR:/root/src" \
       -v "$ANDROID_CARGO_REGISTRY_DIR:/usr/local/cargo/registry" \
       -v "$ANDROID_CARGO_GIT_DIR:/usr/local/cargo/git" \
       -v "$ANDROID_RUSTUP_HOME:/usr/local/rustup" \
       -v "$ANDROID_DOT_ANDROID_DIR:/root/.android" \
       -w /root/src \
       notfl3/cargo-apk \
       sh -lc 'set -e
if ! rustup toolchain list | grep -Eq "^${ANDROID_RUST_TOOLCHAIN}(-| )"; then
    rustup toolchain install "$ANDROID_RUST_TOOLCHAIN" --profile minimal
fi
rustup target add --toolchain "$ANDROID_RUST_TOOLCHAIN" aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
rustup default "$ANDROID_RUST_TOOLCHAIN"
manifest_hash=$(sha256sum Cargo.toml | awk "{print \$1}")
fetch_stamp=/usr/local/cargo/registry/.ridi-app-fetch-$manifest_hash
if [ ! -f "$fetch_stamp" ]; then
    cargo fetch --target aarch64-linux-android --target armv7-linux-androideabi --target i686-linux-android --target x86_64-linux-android
    touch "$fetch_stamp"
fi
legacy_registry=github.com-1ecc6299db9ec823
legacy_cache=/usr/local/cargo/registry/cache/$legacy_registry
legacy_src=/usr/local/cargo/registry/src/$legacy_registry
mkdir -p "$legacy_cache" "$legacy_src"
find /usr/local/cargo/registry/cache -name "*.crate" -exec cp -n {} "$legacy_cache" \;
for crate in "$legacy_cache"/*.crate; do
    package=$(basename "$crate" .crate)
    if [ ! -d "$legacy_src/$package" ]; then
        tar -xzf "$crate" -C "$legacy_src"
        touch "$legacy_src/$package/.cargo-ok"
    fi
done
rm -f Cargo.lock
build_profile_arg=""
if [ "$ANDROID_APP_BUILD_PROFILE" = "release" ]; then
    build_profile_arg="--release"
fi
for attempt in $(seq 1 20); do
    find /usr/local/cargo/registry/src -name Cargo.toml -exec sed -i "s/^edition = \"2024\"$/edition = \"2021\"/" {} +
    if cargo quad-apk build $build_profile_arg --offline; then
        exit 0
    fi
    echo "cargo quad-apk failed; patched any newly downloaded 2024-edition manifests and will retry ($attempt/20)" >&2
done
exit 1'

    echo "Android $profile APK: $ANDROID_APP_BUILD_DIR/target/android-artifacts/$apk_profile_dir/apk/app.apk"
}

cmd_app_debug_apk_path() {
    printf '%s\n' "$SCRIPT_DIR/target/android-ridi-app/target/android-artifacts/debug/apk/app.apk"
}

cmd_app_require_adb() {
    if ! command -v adb >/dev/null 2>&1; then
        echo "Error: adb not found. Install Android platform tools, e.g. on Arch: sudo pacman -S android-tools" >&2
        exit 1
    fi
}

cmd_app_logcat() {
    cmd_app_require_adb
    adb logcat -v time | grep -E --line-buffered 'AndroidRuntime|FATAL EXCEPTION|DEBUG|libc|ridi|Ridi|bike\.ridi\.app|RustStdoutStderr'
}

cmd_app_run_debug() {
    local apk
    apk="$(cmd_app_debug_apk_path)"

    cmd_app_require_adb
    cmd_app_build debug

    if [[ ! -f "$apk" ]]; then
        echo "Error: debug APK not found: $apk" >&2
        exit 1
    fi

    adb devices
    if ! adb install -r "$apk"; then
        echo "Install failed; uninstalling existing debug app and retrying." >&2
        adb uninstall bike.ridi.app || true
        adb install "$apk"
    fi
    adb logcat -c
    adb shell monkey -p bike.ridi.app -c android.intent.category.LAUNCHER 1
    echo "Streaming logs. Press Ctrl-C to stop."
    cmd_app_logcat
}

RUST_ROUTER_CRATE="$SCRIPT_DIR/crates/ridi-router-mobile"
RUST_ROUTER_UDL="$RUST_ROUTER_CRATE/src/ridi_router_mobile.udl"
RUST_ROUTER_ANDROID_DIR="$SCRIPT_DIR/apps/ridi-mobile/modules/ridi-router/android"
RUST_ROUTER_IOS_DIR="$SCRIPT_DIR/apps/ridi-mobile/modules/ridi-router/ios"
RUST_ROUTER_IOS_RUST_DIR="$RUST_ROUTER_IOS_DIR/rust"
RUST_ROUTER_TARGET_DIR="$SCRIPT_DIR/target/ridi-router-mobile"

require_command() {
    local command_name="$1"
    local install_hint="${2:-}"

    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "Error: $command_name not found.$install_hint" >&2
        exit 1
    fi
}

rust_target_installed() {
    local target="$1"
    rustup target list --installed | grep -Fxq "$target"
}

require_rust_targets() {
    local target
    for target in "$@"; do
        if ! rust_target_installed "$target"; then
            echo "Error: Rust target '$target' is not installed. Run: rustup target add $*" >&2
            exit 1
        fi
    done
}

cmd_rust_router_bindings() {
    local out_dir="$RUST_ROUTER_TARGET_DIR/bindings"
    local kotlin_out="$out_dir/kotlin"
    local swift_out="$out_dir/swift"

    rm -rf "$out_dir"
    mkdir -p "$kotlin_out" "$swift_out"

    cargo run -p ridi-router-mobile --features uniffi/cli --bin ridi-router-mobile-uniffi-bindgen -- \
        generate "$RUST_ROUTER_UDL" --language kotlin --out-dir "$kotlin_out"
    cargo run -p ridi-router-mobile --features uniffi/cli --bin ridi-router-mobile-uniffi-bindgen -- \
        generate "$RUST_ROUTER_UDL" --language swift --out-dir "$swift_out"

    rm -rf "$RUST_ROUTER_ANDROID_DIR/src/main/java/uniffi/ridi_router_mobile"
    mkdir -p "$RUST_ROUTER_ANDROID_DIR/src/main/java/uniffi/ridi_router_mobile"
    find "$kotlin_out" -type f -name '*.kt' -exec cp {} "$RUST_ROUTER_ANDROID_DIR/src/main/java/uniffi/ridi_router_mobile/" \;

    mkdir -p "$RUST_ROUTER_IOS_RUST_DIR"
    find "$RUST_ROUTER_IOS_DIR" -maxdepth 1 -type f \( -name 'ridi_router_mobile.swift' -o -name 'ridi_router_mobileFFI.*' -o -name '*.modulemap' \) -delete
    find "$swift_out" -maxdepth 1 -type f \( -name '*.swift' -o -name '*.h' -o -name '*.modulemap' \) -exec cp {} "$RUST_ROUTER_IOS_DIR/" \;

    echo "Generated UniFFI bindings in Expo module native source trees."
}

cmd_rust_router_android() {
    local android_targets=(
        aarch64-linux-android
        armv7-linux-androideabi
        i686-linux-android
        x86_64-linux-android
    )

    require_command cargo-ndk " Install with: cargo install cargo-ndk"
    require_rust_targets "${android_targets[@]}"

    if [[ -n "${NDK_HOME:-}" ]]; then
        export NDK_HOME="${NDK_HOME%/}"
    fi
    if [[ -n "${ANDROID_NDK_HOME:-}" ]]; then
        export ANDROID_NDK_HOME="${ANDROID_NDK_HOME%/}"
    fi

    cargo ndk -t arm64-v8a -t armeabi-v7a -t x86 -t x86_64 -P 24 -o "$RUST_ROUTER_ANDROID_DIR/src/main/jniLibs" \
        build -p ridi-router-mobile --release

    echo "Copied Android Rust libraries to $RUST_ROUTER_ANDROID_DIR/src/main/jniLibs"
}

cmd_rust_router_ios() {
    local ios_targets=(
        aarch64-apple-ios
        aarch64-apple-ios-sim
        x86_64-apple-ios
    )

    require_command xcodebuild " Install Xcode command line tools."
    require_command lipo " Install Xcode command line tools."
    require_rust_targets "${ios_targets[@]}"

    cargo build -p ridi-router-mobile --release --target aarch64-apple-ios
    cargo build -p ridi-router-mobile --release --target aarch64-apple-ios-sim
    cargo build -p ridi-router-mobile --release --target x86_64-apple-ios

    local build_dir="$RUST_ROUTER_TARGET_DIR/ios"
    local sim_universal_dir="$build_dir/simulator-universal"
    local headers_dir="$build_dir/headers"
    local framework_dir="$RUST_ROUTER_IOS_RUST_DIR/RidiRouterMobile.xcframework"

    rm -rf "$build_dir" "$framework_dir"
    mkdir -p "$sim_universal_dir" "$headers_dir" "$RUST_ROUTER_IOS_RUST_DIR"

    lipo -create \
        "$SCRIPT_DIR/target/aarch64-apple-ios-sim/release/libridi_router_mobile.a" \
        "$SCRIPT_DIR/target/x86_64-apple-ios/release/libridi_router_mobile.a" \
        -output "$sim_universal_dir/libridi_router_mobile.a"

    find "$RUST_ROUTER_IOS_DIR" -maxdepth 1 -type f -name '*.h' -exec cp {} "$headers_dir/" \;
    if ! find "$headers_dir" -type f -name '*.h' | grep -q .; then
        printf '%s\n' '/* UniFFI headers are generated by ./dev.sh rust-router bindings. */' > "$headers_dir/RidiRouterMobile.h"
    fi

    xcodebuild -create-xcframework \
        -library "$SCRIPT_DIR/target/aarch64-apple-ios/release/libridi_router_mobile.a" -headers "$headers_dir" \
        -library "$sim_universal_dir/libridi_router_mobile.a" -headers "$headers_dir" \
        -output "$framework_dir"

    echo "Copied iOS Rust xcframework to $framework_dir"
}

cmd_rust_router() {
    local subcommand="${1:-}"

    case "$subcommand" in
        android)
            cmd_rust_router_android
            ;;
        ios)
            cmd_rust_router_ios
            ;;
        bindings)
            cmd_rust_router_bindings
            ;;
        build)
            cmd_rust_router_bindings
            cmd_rust_router_android
            cmd_rust_router_ios
            ;;
        *)
            echo "Usage: ./dev.sh rust-router {android|ios|bindings|build}" >&2
            exit 1
            ;;
    esac
}

cmd_mobile() {
    local platform="${1:-}"
    local target="${2:-}"

    case "$platform:$target" in
        android:device)
            pnpm --dir "$SCRIPT_DIR/apps/ridi-mobile" android -- --device
            ;;
        android:simulator)
            pnpm --dir "$SCRIPT_DIR/apps/ridi-mobile" android
            ;;
        ios:device)
            pnpm --dir "$SCRIPT_DIR/apps/ridi-mobile" ios -- --device
            ;;
        ios:simulator)
            pnpm --dir "$SCRIPT_DIR/apps/ridi-mobile" ios -- --simulator
            ;;
        *)
            echo "Usage: ./dev.sh mobile {android|ios} {device|simulator}" >&2
            exit 1
            ;;
    esac
}

# Main
case "${1:-}" in
    rust-router)
        shift
        cmd_rust_router "$@"
        ;;
    mobile)
        shift
        cmd_mobile "$@"
        ;;
    app-build)
        cmd_app_build
        ;;
    app-build-debug)
        cmd_app_build debug
        ;;
    app-run-debug)
        cmd_app_run_debug
        ;;
    app-logcat)
        cmd_app_logcat
        ;;
    pbf)
        download_pbf "${2:-}"
        ;;
    generate-tiles)
        cmd_generate_tiles "${2:-}"
        ;;
    route)
        shift
        cmd_route "$@"
        ;;
    build)
        cmd_build
        ;;
    run)
        shift
        cmd_run "$@"
        ;;
    test)
        cmd_test
        ;;
    rmdf-view)
        cmd_rmdf_view
        ;;
    help|--help|-h)
        cmd_help
        ;;
    *)
        cmd_help
        exit 1
        ;;
esac
