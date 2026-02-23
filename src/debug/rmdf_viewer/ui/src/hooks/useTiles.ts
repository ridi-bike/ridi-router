import { useState, useCallback } from 'react';
import type { ManifestResponse, TileSummary, TileResponse } from '../types';
import { fetchManifest, fetchTile } from '../api/client';

export interface LoadedTile {
  summary: TileSummary;
  data: TileResponse;
  visible: boolean;
}

export function useTiles() {
  const [manifest, setManifest] = useState<ManifestResponse | null>(null);
  const [loadedTiles, setLoadedTiles] = useState<Map<string, LoadedTile>>(new Map());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadManifest = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchManifest();
      setManifest(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load manifest');
    } finally {
      setLoading(false);
    }
  }, []);

  const loadTile = useCallback(async (filename: string) => {
    if (loadedTiles.has(filename)) return; // Already loaded
    
    setLoading(true);
    setError(null);
    try {
      const data = await fetchTile(filename);
      const summary = manifest?.tiles.find(t => t.filename === filename);
      if (!summary) throw new Error('Tile not found in manifest');
      
      setLoadedTiles(prev => {
        const next = new Map(prev);
        next.set(filename, { summary, data, visible: true });
        return next;
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to load tile: ${filename}`);
    } finally {
      setLoading(false);
    }
  }, [manifest, loadedTiles]);

  const toggleVisibility = useCallback((filename: string) => {
    setLoadedTiles(prev => {
      const tile = prev.get(filename);
      if (!tile) return prev;
      
      const next = new Map(prev);
      next.set(filename, { ...tile, visible: !tile.visible });
      return next;
    });
  }, []);

  return {
    manifest,
    loadedTiles,
    loading,
    error,
    loadManifest,
    loadTile,
    toggleVisibility,
  };
}
