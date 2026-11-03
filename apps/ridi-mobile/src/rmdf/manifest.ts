import * as FileSystem from 'expo-file-system/legacy';

import { NETWORK_DISRUPTED_WARNING, RMDF_BASE_URL, RMDF_INDEX_URL, RMDF_SUPPORTED_FORMAT_VERSION } from './constants';
import { getRmdfReleaseCacheDirectory, readTextIfExists } from './storage';
import type { RmdfBounds, RmdfChunkManifest, RmdfChunkSummary, RmdfManifestTile, RmdfRelease, RmdfReleaseLoadResult, RmdfTileIndex } from './types';

function isNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function isString(value: unknown): value is string {
  return typeof value === 'string';
}

function parseBounds(value: unknown): RmdfBounds | null {
  if (!value || typeof value !== 'object') return null;
  const bounds = value as Partial<RmdfBounds>;
  if (!isNumber(bounds.lat_min) || !isNumber(bounds.lat_max) || !isNumber(bounds.lon_min) || !isNumber(bounds.lon_max)) return null;
  return { lat_min: bounds.lat_min, lat_max: bounds.lat_max, lon_min: bounds.lon_min, lon_max: bounds.lon_max };
}

function parseTile(value: unknown): RmdfManifestTile | null {
  if (!value || typeof value !== 'object') return null;
  const tile = value as Partial<RmdfManifestTile>;
  const bounds = parseBounds(tile.bounds);
  if (
    !bounds ||
    !isString(tile.filename) ||
    !isString(tile.path) ||
    !isString(tile.checksum) ||
    !isNumber(tile.col) ||
    !isNumber(tile.row) ||
    !isNumber(tile.size_bytes)
  ) {
    return null;
  }
  return {
    filename: tile.filename,
    col: tile.col,
    row: tile.row,
    bounds,
    size_bytes: tile.size_bytes,
    point_count: isNumber(tile.point_count) ? tile.point_count : 0,
    line_count: isNumber(tile.line_count) ? tile.line_count : 0,
    checksum: tile.checksum,
    path: tile.path,
  };
}

function parseChunkSummary(value: unknown): RmdfChunkSummary | null {
  if (!value || typeof value !== 'object') return null;
  const chunk = value as Partial<RmdfChunkSummary>;
  const bounds = parseBounds(chunk.bounds);
  if (!bounds || !isString(chunk.id) || !isString(chunk.manifest) || !isNumber(chunk.tile_count) || !isNumber(chunk.total_bytes)) return null;
  const tileBounds = parseBounds(chunk.tile_bounds) ?? undefined;
  return { id: chunk.id, bounds, tile_bounds: tileBounds, tile_count: chunk.tile_count, total_bytes: chunk.total_bytes, manifest: chunk.manifest };
}

export function parseRmdfIndex(json: unknown): RmdfTileIndex {
  if (!json || typeof json !== 'object') throw new Error('Tile index is not an object.');
  const index = json as Partial<RmdfTileIndex>;
  if (!Array.isArray(index.releases)) throw new Error('Tile index has no releases.');
  const releases = index.releases.map((value): RmdfRelease => {
    const release = value as Partial<RmdfRelease>;
    if (!isNumber(release.format_version) || !isString(release.release_id) || !isNumber(release.tile_size_degrees) || !Array.isArray(release.chunks)) {
      throw new Error('Tile index contains an invalid release.');
    }
    const chunks = release.chunks.map(parseChunkSummary);
    if (chunks.some((chunk) => chunk === null)) throw new Error(`Release ${release.release_id} has invalid chunks.`);
    return {
      format_version: release.format_version,
      release_id: release.release_id,
      published_at: isString(release.published_at) ? release.published_at : '',
      tile_size_degrees: release.tile_size_degrees,
      chunk_size_degrees: isNumber(release.chunk_size_degrees) ? release.chunk_size_degrees : 10,
      tile_count: isNumber(release.tile_count) ? release.tile_count : 0,
      total_bytes: isNumber(release.total_bytes) ? release.total_bytes : 0,
      source_files: Array.isArray(release.source_files) ? release.source_files.filter(isString) : [],
      chunks: chunks as RmdfChunkSummary[],
    };
  });
  return { index_version: isNumber(index.index_version) ? index.index_version : 0, releases };
}

export function parseRmdfChunkManifest(json: unknown): RmdfChunkManifest {
  if (!json || typeof json !== 'object') throw new Error('Chunk manifest is not an object.');
  const chunk = json as Partial<RmdfChunkManifest>;
  if (!Array.isArray(chunk.tiles)) throw new Error('Chunk manifest tiles are missing.');
  const tiles = chunk.tiles.map(parseTile);
  if (tiles.some((tile) => tile === null)) throw new Error('Chunk manifest contains invalid tile entries.');
  return {
    format_version: isNumber(chunk.format_version) ? chunk.format_version : 0,
    release_id: isString(chunk.release_id) ? chunk.release_id : '',
    tile_size_degrees: isNumber(chunk.tile_size_degrees) ? chunk.tile_size_degrees : 0,
    tiles: tiles as RmdfManifestTile[],
  };
}

function selectRelease(index: RmdfTileIndex): RmdfRelease {
  const release = index.releases.find((candidate) => candidate.format_version === RMDF_SUPPORTED_FORMAT_VERSION);
  if (!release) {
    throw new Error(`No map data published for routing format v${RMDF_SUPPORTED_FORMAT_VERSION}. Update the app.`);
  }
  return release;
}

async function fetchJson(url: string): Promise<unknown> {
  const response = await fetch(url, { cache: 'no-store' } as RequestInit);
  if (!response.ok) throw new Error(`${url} failed with HTTP ${response.status}.`);
  return response.json();
}

async function ensureReleaseCacheDirectory(subdirectory = ''): Promise<string> {
  const directory = `${getRmdfReleaseCacheDirectory()}${subdirectory}`;
  const info = await FileSystem.getInfoAsync(directory);
  if (!info.exists) await FileSystem.makeDirectoryAsync(directory, { intermediates: true });
  return directory;
}

function cachedIndexPath(): string {
  return `${getRmdfReleaseCacheDirectory()}index.json`;
}

function cachedChunkPath(releaseId: string, chunkId: string): string {
  return `${getRmdfReleaseCacheDirectory()}${releaseId}/${chunkId}.json`;
}

/** Fetches the current release, falling back to the last one seen when offline. */
export async function loadRmdfRelease(): Promise<RmdfReleaseLoadResult> {
  try {
    const json = await fetchJson(RMDF_INDEX_URL);
    const release = selectRelease(parseRmdfIndex(json));
    await ensureReleaseCacheDirectory();
    await FileSystem.writeAsStringAsync(cachedIndexPath(), JSON.stringify(json));
    return { release, source: 'remote', deleteOnly: false };
  } catch (remoteError) {
    const text = await readTextIfExists(cachedIndexPath()).catch(() => null);
    if (text) {
      return { release: selectRelease(parseRmdfIndex(JSON.parse(text))), source: 'cache', deleteOnly: true, warning: NETWORK_DISRUPTED_WARNING };
    }
    throw remoteError instanceof Error ? remoteError : new Error('Tile index request failed and no cache exists.');
  }
}

const chunkRequests = new Map<string, Promise<RmdfChunkManifest>>();

/** Chunk manifests are immutable per release, so they are cached on disk and in memory. */
export function loadRmdfChunk(release: RmdfRelease, chunk: RmdfChunkSummary): Promise<RmdfChunkManifest> {
  const key = `${release.release_id}/${chunk.id}`;
  let request = chunkRequests.get(key);
  if (!request) {
    request = loadChunkUncached(release.release_id, chunk);
    request.catch(() => chunkRequests.delete(key));
    chunkRequests.set(key, request);
  }
  return request;
}

async function loadChunkUncached(releaseId: string, chunk: RmdfChunkSummary): Promise<RmdfChunkManifest> {
  const path = cachedChunkPath(releaseId, chunk.id);
  const cached = await readTextIfExists(path).catch(() => null);
  if (cached) return parseRmdfChunkManifest(JSON.parse(cached));
  const json = await fetchJson(`${RMDF_BASE_URL}${chunk.manifest}`);
  const manifest = parseRmdfChunkManifest(json);
  await ensureReleaseCacheDirectory(`${releaseId}/`);
  await FileSystem.writeAsStringAsync(path, JSON.stringify(json));
  return manifest;
}

export function getRmdfTileUrl(tilePath: string): string {
  return `${RMDF_BASE_URL}${tilePath}`;
}
