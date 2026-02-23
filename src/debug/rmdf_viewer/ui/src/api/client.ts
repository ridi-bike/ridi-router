import type { ManifestResponse, TileResponse } from '../types';

const API_BASE = '/api';

export async function fetchManifest(): Promise<ManifestResponse> {
  const response = await fetch(`${API_BASE}/manifest`);
  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.error || 'Failed to fetch manifest');
  }
  return response.json();
}

export async function fetchTile(filename: string): Promise<TileResponse> {
  const response = await fetch(`${API_BASE}/tiles/${filename}`);
  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.error || `Failed to fetch tile: ${filename}`);
  }
  return response.json();
}
