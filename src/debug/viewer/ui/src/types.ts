import { DebugStreamItineraries, DebugStreamSteps } from "./api-types.js";
import type turf from "@turf/turf";
import * as maplibregl from "maplibre-gl";

export type MapActions = {
  current: null | {
    addMarker: (params: {
      id: string;
      lat: number;
      lon: number;
      markerName: string;
    }) => void;
    removeMarkers: () => void;
    removePoints: () => void;
    addPoint: (params: {
      id: string;
      lat: number;
      lon: number;
      pointName: string;
      radius: null | number;
    }) => void;
    points: string[];
    markers: Map<string, maplibregl.Marker>;
    setCenter: (center: maplibregl.LngLatLike) => void;
    setView: (bbox: ReturnType<typeof turf.bbox>) => void;
    addRoute: (id: string, route: [number, number][], color: string) => void;
    routes: string[];
    removeRoutes: () => void;
  };
};

export type PaginationClick = () => void;

export type SelectionState = {
  itinerary: null | DebugStreamItineraries;
  step: null | DebugStreamSteps;
};
