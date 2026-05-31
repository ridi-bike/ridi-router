import { getRmdfTilePath } from './storage';
import type { RmdfGeoJsonFeatureCollection, RmdfManifest, RmdfManifestTile, RmdfTileStatus, RmdfTileViewModel } from './types';

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 MB';
  const gb = bytes / 1024 / 1024 / 1024;
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  const mb = bytes / 1024 / 1024;
  return `${mb.toFixed(1)} MB`;
}

export function getTileCenter(tile: RmdfManifestTile): [number, number] {
  return [
    (tile.bounds.lon_min + tile.bounds.lon_max) / 2,
    (tile.bounds.lat_min + tile.bounds.lat_max) / 2,
  ];
}

export function getManifestCenter(manifest: RmdfManifest): [number, number] {
  if (manifest.tiles.length === 0) return [11.5761, 48.1372];
  let latMin = Infinity;
  let latMax = -Infinity;
  let lonMin = Infinity;
  let lonMax = -Infinity;
  for (const tile of manifest.tiles) {
    latMin = Math.min(latMin, tile.bounds.lat_min);
    latMax = Math.max(latMax, tile.bounds.lat_max);
    lonMin = Math.min(lonMin, tile.bounds.lon_min);
    lonMax = Math.max(lonMax, tile.bounds.lon_max);
  }
  return [(lonMin + lonMax) / 2, (latMin + latMax) / 2];
}

export function hitTestTile(tiles: RmdfManifestTile[], longitude: number, latitude: number): RmdfManifestTile | null {
  return (
    tiles.find(
      (tile) =>
        longitude >= tile.bounds.lon_min &&
        longitude <= tile.bounds.lon_max &&
        latitude >= tile.bounds.lat_min &&
        latitude <= tile.bounds.lat_max
    ) ?? null
  );
}

export function buildTileViewModels(args: {
  tiles: RmdfManifestTile[];
  downloadedIds: Set<string>;
  selectedIds: Set<string>;
  queuedIds: Set<string>;
  downloadingIds: Set<string>;
  failedIds: Set<string>;
  queuePositions: Map<string, number>;
}): RmdfTileViewModel[] {
  return args.tiles.map((tile) => {
    let status: RmdfTileStatus = 'remoteOnly';
    if (args.failedIds.has(tile.filename)) status = 'failed';
    if (args.downloadedIds.has(tile.filename)) status = 'downloaded';
    if (args.queuedIds.has(tile.filename)) status = 'queued';
    if (args.downloadingIds.has(tile.filename)) status = 'downloading';

    return {
      id: tile.filename,
      manifestTile: tile,
      selected: args.selectedIds.has(tile.filename),
      status,
      localPath: getRmdfTilePath(tile.filename),
      queuePosition: args.queuePositions.get(tile.filename),
    };
  });
}

export function tilesToGeoJson(tileViewModels: RmdfTileViewModel[]): RmdfGeoJsonFeatureCollection {
  return {
    type: 'FeatureCollection',
    features: tileViewModels.map((tile) => ({
      type: 'Feature',
      id: tile.id,
      properties: {
        id: tile.id,
        filename: tile.id,
        status: tile.status,
        selected: tile.selected,
        queuePosition: tile.queuePosition ?? 0,
        label: tile.queuePosition ? String(tile.queuePosition) : '',
      },
      geometry: {
        type: 'Polygon',
        coordinates: [
          [
            [tile.manifestTile.bounds.lon_min, tile.manifestTile.bounds.lat_min],
            [tile.manifestTile.bounds.lon_max, tile.manifestTile.bounds.lat_min],
            [tile.manifestTile.bounds.lon_max, tile.manifestTile.bounds.lat_max],
            [tile.manifestTile.bounds.lon_min, tile.manifestTile.bounds.lat_max],
            [tile.manifestTile.bounds.lon_min, tile.manifestTile.bounds.lat_min],
          ],
        ],
      },
    })),
  };
}

export function splitSelectedTiles(tiles: RmdfManifestTile[], selectedIds: Set<string>, downloadedIds: Set<string>) {
  const selectedTiles = tiles.filter((tile) => selectedIds.has(tile.filename));
  const selectedMissingTiles = selectedTiles.filter((tile) => !downloadedIds.has(tile.filename));
  const selectedDownloadedTiles = selectedTiles.filter((tile) => downloadedIds.has(tile.filename));
  const selectedMissingBytes = selectedMissingTiles.reduce((sum, tile) => sum + tile.size_bytes, 0);
  return { selectedTiles, selectedMissingTiles, selectedDownloadedTiles, selectedMissingBytes };
}
