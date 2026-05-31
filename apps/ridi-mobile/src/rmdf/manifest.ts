import { NETWORK_DISRUPTED_WARNING, RMDF_MANIFEST_URL } from './constants';
import { getRmdfManifestPath, readTextIfExists, writeText } from './storage';
import type { RmdfManifest, RmdfManifestLoadResult, RmdfManifestTile } from './types';

function isNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function isString(value: unknown): value is string {
  return typeof value === 'string';
}

function validateTile(value: unknown): RmdfManifestTile | null {
  if (!value || typeof value !== 'object') return null;
  const tile = value as Partial<RmdfManifestTile>;
  const bounds = tile.bounds as Partial<RmdfManifestTile['bounds']> | undefined;
  if (
    !isString(tile.filename) ||
    !isNumber(tile.col) ||
    !isNumber(tile.row) ||
    !bounds ||
    !isNumber(bounds.lat_min) ||
    !isNumber(bounds.lat_max) ||
    !isNumber(bounds.lon_min) ||
    !isNumber(bounds.lon_max) ||
    !isNumber(tile.size_bytes) ||
    !isNumber(tile.point_count) ||
    !isNumber(tile.line_count)
  ) {
    return null;
  }

  return {
    filename: tile.filename,
    col: tile.col,
    row: tile.row,
    bounds: {
      lat_min: bounds.lat_min,
      lat_max: bounds.lat_max,
      lon_min: bounds.lon_min,
      lon_max: bounds.lon_max,
    },
    neighbors: tile.neighbors && typeof tile.neighbors === 'object' ? (tile.neighbors as Record<string, string | null>) : {},
    size_bytes: tile.size_bytes,
    point_count: tile.point_count,
    line_count: tile.line_count,
    checksum: isString(tile.checksum) ? tile.checksum : '',
  };
}

export function parseRmdfManifest(json: unknown): RmdfManifest {
  if (!json || typeof json !== 'object') {
    throw new Error('Manifest JSON is not an object.');
  }
  const manifest = json as Partial<RmdfManifest>;
  if (!Array.isArray(manifest.tiles)) {
    throw new Error('Manifest tiles are missing.');
  }
  const tiles = manifest.tiles.map(validateTile);
  if (tiles.some((tile) => tile === null)) {
    throw new Error('Manifest contains invalid tile entries.');
  }

  return {
    version: isString(manifest.version) ? manifest.version : '',
    tile_size_degrees: isNumber(manifest.tile_size_degrees) ? manifest.tile_size_degrees : 0,
    format_version: isNumber(manifest.format_version) ? manifest.format_version : 0,
    generated_at: isString(manifest.generated_at) ? manifest.generated_at : '',
    source_files: Array.isArray(manifest.source_files) ? manifest.source_files.filter(isString) : [],
    tiles: tiles as RmdfManifestTile[],
  };
}

export async function fetchRemoteManifest(): Promise<RmdfManifest> {
  const response = await fetch(RMDF_MANIFEST_URL);
  if (!response.ok) {
    throw new Error(`Manifest request failed with HTTP ${response.status}.`);
  }
  const json = await response.json();
  return parseRmdfManifest(json);
}

export async function readCachedManifest(): Promise<RmdfManifest | null> {
  const text = await readTextIfExists(getRmdfManifestPath());
  if (!text) return null;
  return parseRmdfManifest(JSON.parse(text));
}

export async function cacheManifest(manifest: RmdfManifest): Promise<void> {
  await writeText(getRmdfManifestPath(), JSON.stringify(manifest));
}

export async function loadRmdfManifest(): Promise<RmdfManifestLoadResult> {
  try {
    const manifest = await fetchRemoteManifest();
    await cacheManifest(manifest);
    return { manifest, source: 'remote', deleteOnly: false };
  } catch (remoteError) {
    const cached = await readCachedManifest();
    if (cached) {
      return {
        manifest: cached,
        source: 'cache',
        deleteOnly: true,
        warning: NETWORK_DISRUPTED_WARNING,
      };
    }
    throw remoteError instanceof Error ? remoteError : new Error('Manifest request failed and no cache exists.');
  }
}
