import "./style.css";

import * as maplibregl from "maplibre-gl";
import * as turf from "@turf/turf";

import van from "vanjs-core";
import {
  DebugStreamItineraries,
  DebugStreamItineraryWaypoints,
} from "./api-types";
import { MapActions, SelectionState } from "./types";

const { button, div, table, td, th, tr } = van.tags;
const selection = van.state<SelectionState>({
  itinerary: null,
});
const itineraries = van.state([] as DebugStreamItineraries[]);
const itineraryWaypoints = van.state([] as DebugStreamItineraryWaypoints[]);
// const steps = van.state([] as DebugStreamSteps[]);
// const stepResults = van.state([] as DebugStreamStepResults[]);
// const forkChoices = van.state([] as DebugStreamForkChoices[]);
// const forkChoiceWeights = van.state([] as DebugStreamForkChoiceWeights[]);
//
const mapActions: MapActions = {
  current: null,
};

const Pagination = (page: number, next: () => void, back: () => void) => {
  return div(
    { class: "flex flex-row" },
    button(
      {
        class: "dark:hover:bg-gray-800 hover:bg-gray-200 px-4 py-1",
        onclick: back,
      },
      "<",
    ),
    div({ class: "px-4 py-1" }, page),
    button(
      {
        class: "dark:hover:bg-gray-800 hover:bg-gray-200 px-4 py-1",
        onclick: next,
      },
      ">",
    ),
  );
};

const Itineraries = () => {
  const pageSize = 20;
  const page = van.state(0);

  van.derive(() =>
    fetch(
      `http://0.0.0.0:1337/data/DebugStreamItineraries?limit=${pageSize}&offset=${page.val * pageSize}`,
    )
      .then((req) => req.json())
      .then((data) => (itineraries.val = data))
      .catch(console.error),
  );

  return div(
    Pagination(
      page.val,
      () => page.val++,
      () => (page.val > 0 ? page.val-- : void 0),
    ),
    () =>
      table([
        tr([th("num"), th("id"), th("wps"), th("radius"), th("visit_all")]),
        ...itineraries.val.map((it, idx) =>
          tr([
            td(page.val * pageSize + idx + 1),
            td(
              button(
                {
                  class: "dark:hover:bg-gray-800 hover:bg-gray-200",
                  onclick: () => {
                    selection.val = {
                      ...selection.val,
                      itinerary: it,
                    };
                  },
                },
                it.itinerary_id,
              ),
            ),
            td(it.waypoints_count),
            td(it.radius),
            td(it.visit_all),
          ]),
        ),
      ]),
  );
};

const ItineraryWaypoints = () => {
  const pageSize = 20;
  const page = van.state(0);

  van.derive(
    () =>
      !!selection.val.itinerary &&
      fetch(
        `http://0.0.0.0:1337/data/DebugStreamItineraryWaypoints?itinerary_id=${selection.val.itinerary.itinerary_id}&limit=${pageSize}&offset=${page.val * pageSize}`,
      )
        .then((req) => req.json())
        .then((data) => (itineraryWaypoints.val = data))
        .catch(console.error),
  );

  van.derive(() => {
    if (mapActions.current) {
      for (const [_id, marker] of mapActions.current.markers.entries()) {
        marker.remove();
      }
      mapActions.current.markers = new Map();
    }
    if (!selection.val.itinerary) {
      return;
    }
    if (!mapActions.current) {
      return;
    }
    const features = turf.points([
      [selection.val.itinerary.start_lon, selection.val.itinerary.start_lat],
      [selection.val.itinerary.finish_lon, selection.val.itinerary.finish_lat],
      ...itineraryWaypoints.val.map((wp) => [wp.lon, wp.lat]),
    ]);

    const center = turf.center(features);
    mapActions.current.setCenter(
      center.geometry.coordinates as [number, number],
    );

    mapActions.current.addMarker({
      id: "start",
      markerName: "Start",
      lat: selection.val.itinerary.start_lat,
      lon: selection.val.itinerary.start_lon,
    });
    mapActions.current.addMarker({
      id: "finish",
      markerName: "Finish",
      lat: selection.val.itinerary.finish_lat,
      lon: selection.val.itinerary.finish_lon,
    });
    const points = itineraryWaypoints.val
      .map((wp) => {
        if (!mapActions.current) {
          alert("map not loaded");
          return;
        }
        mapActions.current.addPoint({
          id: `${wp.itinerary_id}-${wp.idx}`,
          pointName: `wp-${wp.idx}`,
          lat: wp.lat,
          lon: wp.lon,
          radius: selection.val.itinerary?.radius || 0,
        });
        return [wp.lon, wp.lat];
      })
      .filter(Boolean) as [number, number][];

    const line = turf.lineString(points);
    const bbox = turf.bbox(line);
    mapActions.current.setView(bbox);
    // const bboxPolygon = turf.bboxPolygon(bbox);
  });

  return div(
    Pagination(
      page.val,
      () => page.val++,
      () => (page.val > 0 ? page.val-- : void 0),
    ),
    () =>
      table([
        tr([th("num"), th("itineraryId"), th("idx"), th("lat"), th("lon")]),
        ...itineraryWaypoints.val.map((it, idx) =>
          tr([
            td(page.val * pageSize + idx + 1),
            td(it.itinerary_id),
            td(it.idx),
            td(it.lat),
            td(it.lon),
          ]),
        ),
      ]),
  );
};

const MapContainer = () => {
  const mapContainer = div({ class: "h-96 w-96" });

  var map = new maplibregl.Map({
    container: mapContainer,
    style: "https://basemaps.cartocdn.com/gl/voyager-gl-style/style.json", // style URL
    center: [0, 0], // starting position [lng, lat]
    zoom: 1, // starting zoom
  });

  map.on("load", () => {
    mapActions.current = {
      addPoint: ({ id, lat, lon, pointName, radius }) => {
        map.addSource(id, {
          type: "geojson",
          data: {
            type: "Feature",
            properties: {
              pointName,
              // icon: "music",
            },
            geometry: {
              type: "Point",
              coordinates: [lon, lat],
            },
          },
        });
        map.addLayer({
          id,
          source: id,
          type: "symbol",
          layout: {
            "text-field": ["get", "pointName"],
            "text-variable-anchor": ["top", "bottom", "left", "right"],
            "text-radial-offset": 0.5,
            "text-justify": "auto",
          },
        });
        if (radius) {
          const circle = turf.circle([lon, lat], radius, {
            units: "meters",
            steps: 64,
          });
          map.addSource(`${id}-radius`, {
            type: "geojson",
            data: circle,
          });
          map.addLayer({
            id: `${id}-radius`,
            type: "fill",
            source: `${id}-radius`,
            paint: {
              "fill-color": "#FF0000",
              "fill-opacity": 0.3,
            },
          });
        }
      },
      addMarker: ({ id, lat, lon, markerName }) => {
        console.log("add marker", lat, lon, id);
        const marker = new maplibregl.Marker();
        marker.setLngLat([lon, lat]).addTo(map);
        const popup = new maplibregl.Popup();
        popup.setText(markerName);
        marker.setPopup(popup);
        if (!mapActions.current) {
          throw new Error("not set");
        }
        mapActions.current.markers.set(id, marker);
      },
      setCenter: (center) => {
        map.setCenter(center);
      },
      markers: new Map(),
      points: [],
      setView: (bbox) => {
        map.fitBounds(bbox);
      },
    };
  });

  return mapContainer;
};

const App = () => {
  return div(Itineraries(), ItineraryWaypoints(), MapContainer());
};

van.add(document.body, App());
