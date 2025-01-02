import { DebugStreamItineraries } from "./api-types.js";
import * as maplibregl from "./maplibre-gl.d.ts";
export type MapActions = {
  addMarker:
    | null
    | ((params: {
        id: string;
        lat: number;
        lon: number;
        markerName: string;
      }) => void);
  addPoint:
    | null
    | ((params: {
        id: string;
        lat: number;
        lon: number;
        pointName: string;
        radius: null | number;
      }) => void);
  points: string[];
  markers: Map<string, maplibregl.Marker>;
  setCenter: null | ((center: [number, number]) => void);
};

export type PaginationClick = () => void;

export type SelectionState = {
  itinerary: null | DebugStreamItineraries;
};
