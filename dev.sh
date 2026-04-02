#!/bin/bash
set -e

export RUST_BACKTRACE=1

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PBF_DIR="$SCRIPT_DIR/map-data/pbf"
INPUT_DIR="$SCRIPT_DIR/map-data/input"
OUTPUT_DIR="$SCRIPT_DIR/map-data/output"

# PBF URL mappings
declare -A PBF_URLS
PBF_URLS[latvia]="https://download.geofabrik.de/europe/latvia-latest.osm.pbf"
PBF_URLS[estonia]="https://download.geofabrik.de/europe/estonia-latest.osm.pbf"
PBF_URLS[lithuania]="https://download.geofabrik.de/europe/lithuania-latest.osm.pbf"
PBF_URLS[montenegro]="https://download.geofabrik.de/europe/montenegro-latest.osm.pbf"

cmd_help() {
    echo "Usage: ./dev.sh <command> [args]"
    echo ""
    echo "Commands:"
    echo "  pbf <country>              Download PBF file for a country"
    echo "  generate-tiles <countries> Generate tiles for comma-separated countries"
    echo "  build                      Build the project"
    echo "  run                        Run the project"
    echo "  rmdf-view                  Run the RMDF debug viewer"
    echo "  test                       Run tests"
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
    cargo run --release --features=debug-polygons -- generate-tiles --input-dir "$INPUT_DIR" --output-dir "$OUTPUT_DIR" --tile-size-deg 0.1
}

cmd_build() {
    cargo build
}

cmd_run() {
    cargo run -- "$@"
}

cmd_test() {
    cargo test
}

cmd_rmdf_view() {
    cargo run --features rmdf-viewer --release -- rmdf-viewer --input-dir "$OUTPUT_DIR"
}

# Main
case "${1:-}" in
    pbf)
        download_pbf "${2:-}"
        ;;
    generate-tiles)
        cmd_generate_tiles "${2:-}"
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
