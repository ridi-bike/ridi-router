import * as FileSystem from 'expo-file-system/legacy';

import { NETWORK_DISRUPTED_WARNING, RMDF_BASE_URL, RMDF_INDEX_URL, RMDF_SUPPORTED_FORMAT_VERSION } from './constants';
import { getRmdfReleaseCacheDirectory, readTextIfExists, replaceFile } from './storage';
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

/** The server could not be reached or failed; unlike a bad response, the cache is worth using. */
class UnreachableError extends Error {}

async function fetchJson(url: string): Promise<unknown> {
  let response: Response;
  try {
    response = await fetch(url, { cache: 'no-store' } as RequestInit);
  } catch (error) {
    throw new UnreachableError(error instanceof Error ? error.message : `${url} could not be reached.`);
  }
  if (response.status >= 500) throw new UnreachableError(`${url} failed with HTTP ${response.status}.`);
  if (!response.ok) throw new Error(`${url} failed with HTTP ${response.status}.`);
  return response.json();
}

async function ensureReleaseCacheDirectory(subdirectory = ''): Promise<string> {
  const directory = `${getRmdfReleaseCacheDirectory()}${subdirectory}`;
  const info = await FileSystem.getInfoAsync(directory);
  if (!info.exists) await FileSystem.makeDirectoryAsync(directory, { intermediates: true });
  return directory;
}

async function writeCacheFile(path: string, json: unknown): Promise<void> {
  const temp = `${path}.tmp`;
  await FileSystem.writeAsStringAsync(temp, JSON.stringify(json));
  await replaceFile(temp, path);
}

/** Cached JSON, or null when missing or unreadable (a damaged file is removed so it is refetched). */
async function readCacheFile(path: string): Promise<unknown> {
  const text = await readTextIfExists(path).catch(() => null);
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    await FileSystem.deleteAsync(path, { idempotent: true });
    return null;
  }
}

function cachedIndexPath(): string {
  return `${getRmdfReleaseCacheDirectory()}index.json`;
}

function cachedChunkPath(releaseId: string, chunkId: string): string {
  return `${getRmdfReleaseCacheDirectory()}${releaseId}/${chunkId}.json`;
}

/** Chunk caches of releases other than `releaseId` are never read again. */
async function pruneReleaseCaches(releaseId: string): Promise<void> {
  const directory = getRmdfReleaseCacheDirectory();
  const entries = await FileSystem.readDirectoryAsync(directory).catch(() => [] as string[]);
  for (const entry of entries) {
    if (entry === releaseId || entry.endsWith('.json') || entry.endsWith('.tmp')) continue;
    await FileSystem.deleteAsync(`${directory}${entry}`, { idempotent: true }).catch(() => undefined);
  }
}

/**
 * Fetches the current release. Only when the server cannot be reached does it fall back to the
 * last release seen (downloads then stay disabled); a reachable server without a release for this
 * app's format is an error, not "offline".
 */
export async function loadRmdfRelease(): Promise<RmdfReleaseLoadResult> {
  let json: unknown;
  try {
    json = await fetchJson(RMDF_INDEX_URL);
  } catch (error) {
    if (!(error instanceof UnreachableError)) throw error;
    const cached = await readCacheFile(cachedIndexPath());
    if (!cached) throw error;
    return { release: selectRelease(parseRmdfIndex(cached)), source: 'cache', deleteOnly: true, warning: NETWORK_DISRUPTED_WARNING };
  }
  const release = selectRelease(parseRmdfIndex(json));
  await ensureReleaseCacheDirectory();
  await writeCacheFile(cachedIndexPath(), json);
  await pruneReleaseCaches(release.release_id);
  return { release, source: 'remote', deleteOnly: false };
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
  const cached = await readCacheFile(path);
  if (cached) return parseRmdfChunkManifest(cached);
  const json = await fetchJson(`${RMDF_BASE_URL}${chunk.manifest}`);
  const manifest = parseRmdfChunkManifest(json);
  await ensureReleaseCacheDirectory(`${releaseId}/`);
  await writeCacheFile(path, json);
  return manifest;
}

export function getRmdfTileUrl(tilePath: string): string {
  return `${RMDF_BASE_URL}${tilePath}`;
}
