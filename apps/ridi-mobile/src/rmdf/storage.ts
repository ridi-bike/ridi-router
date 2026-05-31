import * as FileSystem from 'expo-file-system/legacy';

import { RMDF_DIRECTORY_NAME, RMDF_MANIFEST_FILENAME } from './constants';
import type { RmdfManifestTile } from './types';

export function getRmdfDirectory(): string {
  if (!FileSystem.documentDirectory) {
    throw new Error('App document directory is unavailable.');
  }
  return `${FileSystem.documentDirectory}${RMDF_DIRECTORY_NAME}/`;
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

export async function getDownloadedTileIds(tiles: RmdfManifestTile[]): Promise<Set<string>> {
  await ensureRmdfDirectory();
  const downloaded = new Set<string>();
  await Promise.all(
    tiles.map(async (tile) => {
      const info = await FileSystem.getInfoAsync(getRmdfTilePath(tile.filename));
      if (info.exists && !info.isDirectory && info.size === tile.size_bytes) {
        downloaded.add(tile.filename);
      }
    })
  );
  return downloaded;
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
