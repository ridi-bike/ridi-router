#!/bin/bash
set -e

export RUST_BACKTRACE=1

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PBF_DIR="$SCRIPT_DIR/map-data/pbf"
INPUT_DIR="$SCRIPT_DIR/map-data/input"
OUTPUT_DIR="$SCRIPT_DIR/map-data/output"
ROUTE_DIR="$SCRIPT_DIR/map-data/routes"
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
    echo "  route <start> <finish> [preset]     Generate a GPX route using OSM place lookup"
    echo "  build                               Build the project"
    echo "  run                                 Run the CLI"
    echo "  rmdf-view                           Run the RMDF debug viewer"
    echo "  test                                Run tests"
    echo ""
    echo "Examples:"
    echo "  ./dev.sh generate-tiles latvia"
    echo "  ./dev.sh route riga,latvia cesis,latvia"
    echo "  ./dev.sh route riga,latvia cesis,latvia prefer-unpaved"
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
    local start_input="${1:-}"
    local finish_input="${2:-}"
    local preset="${3:-}"
    local rule_file=""
    local start_result
    local finish_result
    local start_coords
    local finish_coords
    local start_name
    local finish_name
    local -a route_args

    if [[ -z "$start_input" || -z "$finish_input" ]]; then
        echo "Error: Missing route locations. Usage: ./dev.sh route riga,latvia cesis,latvia [preset]" >&2
        exit 1
    fi

    if [[ $# -gt 3 ]]; then
        echo "Error: Too many arguments. Usage: ./dev.sh route riga,latvia cesis,latvia [preset]" >&2
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

    route_args=(
        generate-route
        --tiles "$OUTPUT_DIR"
        --output-dir "$ROUTE_DIR"
        --format gpx
    )

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

# Main
case "${1:-}" in
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
