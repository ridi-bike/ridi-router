import { hitTestTile } from '@/rmdf/tiles';
import type { RmdfManifestTile } from '@/rmdf/types';
import type { RoutePoint } from './types';

export type TileCoverageResult =
  | { ok: true; tileId: string; tile: RmdfManifestTile }
  | { ok: false; reason: 'missing_manifest_tile' | 'not_downloaded' };

export function validateDownloadedTileCoverage(args: {
  tiles: RmdfManifestTile[];
  downloadedTileIds: Set<string>;
  latitude: number;
  longitude: number;
}): TileCoverageResult {
  const tile = hitTestTile(args.tiles, args.longitude, args.latitude);
  if (!tile) return { ok: false, reason: 'missing_manifest_tile' };
  if (!args.downloadedTileIds.has(tile.filename)) return { ok: false, reason: 'not_downloaded' };
  return { ok: true, tileId: tile.filename, tile };
}

export function pointWithTile(point: RoutePoint, coverage: TileCoverageResult): RoutePoint | null {
  return coverage.ok ? { ...point, tileId: coverage.tileId } : null;
}

export const MISSING_TILE_WARNING = 'No downloaded routing tile exists for this point.';
export const MISSING_START_TILE_WARNING = 'No downloaded routing tile exists for the start point.';
export const MISSING_FINISH_TILE_WARNING = 'No downloaded routing tile exists for the finish point.';
