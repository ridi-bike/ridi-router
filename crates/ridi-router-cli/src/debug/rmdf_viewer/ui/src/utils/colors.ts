// Color scheme for elements based on flags
export const COLORS = {
  // Default colors
  point: '#3388ff',      // Blue
  line: '#3388ff',       // Blue
  
  // Flag-based colors (take precedence)
  residentialInProximity: '#ff9800',  // Orange
  nogoArea: '#f44336',                // Red
  
  // Direction arrow color
  arrow: '#333333',
} as const;

/**
 * Get color for a point based on its flags
 */
export function getPointColor(flags: string[]): string {
  if (flags.includes('nogo_area')) {
    return COLORS.nogoArea;
  }
  if (flags.includes('residential_in_proximity')) {
    return COLORS.residentialInProximity;
  }
  return COLORS.point;
}

/**
 * Get color for a line based on connected point flags
 * Lines inherit the most severe flag from their endpoints
 */
export function getLineColor(pointAFlags: string[], pointBFlags: string[]): string {
  // Check for nogo_area first (most severe)
  if (pointAFlags.includes('nogo_area') || pointBFlags.includes('nogo_area')) {
    return COLORS.nogoArea;
  }
  // Then residential_in_proximity
  if (pointAFlags.includes('residential_in_proximity') || 
      pointBFlags.includes('residential_in_proximity')) {
    return COLORS.residentialInProximity;
  }
  return COLORS.line;
}
