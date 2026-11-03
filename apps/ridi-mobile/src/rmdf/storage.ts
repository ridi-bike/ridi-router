import * as FileSystem from 'expo-file-system/legacy';

import { RMDF_DIRECTORY_NAME, RMDF_MANIFEST_FILENAME, RMDF_RELEASE_CACHE_DIRECTORY_NAME } from './constants';
import type { RmdfManifestTile } from './types';

export function getRmdfDirectory(): string {
  if (!FileSystem.documentDirectory) {
    throw new Error('App document directory is unavailable.');
  }
  return `${FileSystem.documentDirectory}${RMDF_DIRECTORY_NAME}/`;
}

/** Cached index and chunk manifests; kept outside the tiles directory the router reads. */
export function getRmdfReleaseCacheDirectory(): string {
  if (!FileSystem.documentDirectory) {
    throw new Error('App document directory is unavailable.');
  }
  return `${FileSystem.documentDirectory}${RMDF_RELEASE_CACHE_DIRECTORY_NAME}/`;
}

export function getRmdfNativeDirectoryPath(): string {
  const directory = getRmdfDirectory();
  if (directory.startsWith('file://')) {
    return decodeURIComponent(directory.slice('file://'.length));
  }
  return directory;
}

export function getRmdfManifestPath(): string {
  return `${getRmdfDirectory()}${RMDF_MANIFEST_FILENAME}`;
}

export function getRmdfTilePath(filename: string): string {
  return `${getRmdfDirectory()}${filename}`;
}

export function getRmdfTempTilePath(filename: string): string {
  return `${getRmdfDirectory()}${filename}.tmp`;
}

export async function ensureRmdfDirectory(): Promise<string> {
  const directory = getRmdfDirectory();
  const info = await FileSystem.getInfoAsync(directory);
  if (!info.exists) {
    await FileSystem.makeDirectoryAsync(directory, { intermediates: true });
  }
  return directory;
}

export async function readTextIfExists(path: string): Promise<string | null> {
  const info = await FileSystem.getInfoAsync(path);
  if (!info.exists) {
    return null;
  }
  return FileSystem.readAsStringAsync(path);
}

export async function writeText(path: string, value: string): Promise<void> {
  await ensureRmdfDirectory();
  await FileSystem.writeAsStringAsync(path, value);
}

export async function listLocalRmdfFiles(): Promise<Set<string>> {
  await ensureRmdfDirectory();
  const files = await FileSystem.readDirectoryAsync(getRmdfDirectory());
  return new Set(files.filter((file) => file.endsWith('.rmdf')));
}

export async function deleteRmdfTiles(filenames: string[]): Promise<{ deleted: string[]; failed: string[] }> {
  await ensureRmdfDirectory();
  const deleted: string[] = [];
  const failed: string[] = [];

  for (const filename of filenames) {
    try {
      const path = getRmdfTilePath(filename);
      const info = await FileSystem.getInfoAsync(path);
      if (info.exists) {
        await FileSystem.deleteAsync(path, { idempotent: true });
      }
      const tmpPath = getRmdfTempTilePath(filename);
      const tmpInfo = await FileSystem.getInfoAsync(tmpPath);
      if (tmpInfo.exists) {
        await FileSystem.deleteAsync(tmpPath, { idempotent: true });
      }
      deleted.push(filename);
    } catch {
      failed.push(filename);
    }
  }

  return { deleted, failed };
}

/**
 * Moves `from` over `to`. Android renames atomically over an existing file, so readers (the native
 * router opening manifest.json or a tile) never see it missing; iOS refuses to overwrite, so there
 * the old file is removed first.
 */
export async function replaceFile(from: string, to: string): Promise<void> {
  try {
    await FileSystem.moveAsync({ from, to });
  } catch {
    await FileSystem.deleteAsync(to, { idempotent: true });
    await FileSystem.moveAsync({ from, to });
  }
}

async function readBytes(path: string, position: number, length: number): Promise<Uint8Array | null> {
  try {
    const base64 = await FileSystem.readAsStringAsync(path, { encoding: FileSystem.EncodingType.Base64, position, length });
    const bytes = Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
    return bytes.length === length ? bytes : null;
  } catch {
    return null;
  }
}

const CHECKSUM_LEN = 16;

/** An RMDF file ends with its xxh3-128 checksum; manifests list it as `xxh3-128:<hex of those bytes>`. */
export async function readTileChecksum(path: string, size: number): Promise<string | null> {
  if (size < CHECKSUM_LEN) return null;
  const bytes = await readBytes(path, size - CHECKSUM_LEN, CHECKSUM_LEN);
  return bytes ? `xxh3-128:${Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')}` : null;
}

/** Whether the file at `path` is exactly this release tile: same size and trailing checksum. */
export async function isTileFile(path: string, tile: Pick<RmdfManifestTile, 'size_bytes' | 'checksum'>): Promise<boolean> {
  const info = await FileSystem.getInfoAsync(path).catch(() => null);
  if (!info?.exists || info.isDirectory || info.size !== tile.size_bytes) return false;
  return (await readTileChecksum(path, info.size)) === tile.checksum;
}

/** RMDF format version from the file header (`RMDF` magic, then a little-endian u32). */
export async function readTileFormatVersion(path: string): Promise<number | null> {
  const bytes = await readBytes(path, 0, 8);
  if (!bytes || String.fromCharCode(...bytes.slice(0, 4)) !== 'RMDF') return null;
  return (bytes[4] | (bytes[5] << 8) | (bytes[6] << 16) | (bytes[7] << 24)) >>> 0;
}
