import "./style.css";
import "maplibre-gl/dist/maplibre-gl.css";

import * as maplibregl from "maplibre-gl";
import * as turf from "@turf/turf";

import van from "vanjs-core";
import {
  DebugStreamItineraries,
  DebugStreamItineraryWaypoints,
  DebugStreamSteps,
} from "./api-types";
import { MapActions, SelectionState } from "./types";

const { button, div, table, td, th, tr } = van.tags;
const selection = van.state<SelectionState>({
  itinerary: null,
  step: null,
});
const itineraries = van.state([] as DebugStreamItineraries[]);
const itineraryWaypoints = van.state([] as DebugStreamItineraryWaypoints[]);
const steps = van.state([] as DebugStreamSteps[]);
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
        tr([
          th("num"),
          th("id"),
          th("wps"),
          th("radius"),
          th("visit_all"),
          th("start"),
          th("finish"),
        ]),
        ...itineraries.val.map((it, idx) =>
          tr(
            {
              class: () =>
                selection.val.itinerary?.itinerary_id === it.itinerary_id
                  ? "bg-red-100"
                  : "",
            },
            [
              td(page.val * pageSize + idx + 1),
              td(
                button(
                  {
                    class: "dark:hover:bg-gray-800 hover:bg-gray-200",
                    onclick: () => {
                      selection.val = {
                        step: null,
                        itinerary: it,
                      };
                      itineraryWaypoints.val = [];
                      steps.val = [];
                    },
                  },
                  it.itinerary_id,
                ),
              ),
              td(it.waypoints_count),
              td(it.radius),
              td(it.visit_all),
              td(`${it.start_lat},${it.start_lon}`),
              td(`${it.finish_lat},${it.finish_lon}`),
            ],
          ),
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
      selection.val.itinerary.itinerary_id !=
        selection.oldVal.itinerary?.itinerary_id &&
      fetch(
        `http://0.0.0.0:1337/data/DebugStreamItineraryWaypoints?itinerary_id=${selection.val.itinerary.itinerary_id}&limit=${pageSize}&offset=${page.val * pageSize}`,
      )
        .then((req) => req.json())
        .then((data) => (itineraryWaypoints.val = data))
        .catch(console.error),
  );

  van.derive(() => {
    if (
      selection.val.itinerary?.itinerary_id ===
        selection.oldVal.itinerary?.itinerary_id &&
      itineraryWaypoints.val == itineraryWaypoints.oldVal
    ) {
      return;
    }
    mapActions.current?.removeMarkers();
    mapActions.current?.removePoints();

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
      .concat([
        [selection.val.itinerary.start_lon, selection.val.itinerary.start_lat],
        [
          selection.val.itinerary.finish_lon,
          selection.val.itinerary.finish_lat,
        ],
      ])
      .filter(Boolean) as [number, number][];

    if (points.length) {
      const line = turf.lineString(points);
      const bbox = turf.bbox(line);
      mapActions.current.setView([
        bbox[0] - 0.1,
        bbox[1] - 0.1,
        bbox[2] + 0.1,
        bbox[3] + 0.1,
      ]);
    }
  });

  return div(
    Pagination(
      page.val,
      () => page.val++,
      () => (page.val > 0 ? page.val-- : void 0),
    ),
    () =>
      table([
        tr([th("num"), th("idx"), th("lat"), th("lon")]),
        ...itineraryWaypoints.val.map((it, idx) =>
          tr([
            td(page.val * pageSize + idx + 1),
            td(it.idx),
            td(it.lat),
            td(it.lon),
          ]),
        ),
      ]),
  );
};

const Steps = () => {
  const pageSize = 20;
  const page = van.state(0);

  van.derive(
    () =>
      !!selection.val.itinerary &&
      (selection.val.itinerary.itinerary_id !=
        selection.oldVal.itinerary?.itinerary_id ||
        page.val !== page.oldVal) &&
      fetch(
        `http://0.0.0.0:1337/data/DebugStreamSteps?itinerary_id=${selection.val.itinerary.itinerary_id}&limit=${pageSize}&offset=${page.val * pageSize}`,
      )
        .then((req) => req.json())
        .then((data) => (steps.val = data))
        .catch(console.error),
  );

  van.derive(() => {
    console.log("yes yes");
    mapActions.current?.removeRoutes();
    !!selection.val.step &&
      selection.val.step.step_num != selection.oldVal.step?.step_num;
    !!selection.val.itinerary &&
      fetch(
        `http://0.0.0.0:1337/calc/route?itinerary_id=${selection.val.itinerary.itinerary_id}&step=${selection.val.step?.step_num}`,
      )
        .then((resp) => resp.json())
        .then((data) => {
          const routeFragments = data as [number, number][][];
          for (const [fragmentIdx, fragment] of routeFragments.entries()) {
            mapActions.current?.addRoute(
              `${selection.val.itinerary?.itinerary_id}-${selection.val.step?.step_num}-${fragmentIdx}-all`,
              fragment,
              "blue",
            );
          }
        });
  });

  return div(
    Pagination(
      page.val,
      () => page.val++,
      () => (page.val > 0 ? page.val-- : void 0),
    ),
    () =>
      table([
        tr([th("step_num"), th("move_result")]),
        ...steps.val.map((step) =>
          tr(
            {
              class: () =>
                selection.val.step?.step_num === step.step_num
                  ? "bg-red-100"
                  : "",
            },
            [
              td(step.step_num),
              td(
                button(
                  {
                    class: "dark:hover:bg-sky-800 hover:bg-sky-200",
                    onclick: () => {
                      console.log("step click");
                      selection.val = {
                        ...selection.val,
                        step,
                      };
                    },
                  },
                  step.move_result,
                ),
              ),
            ],
          ),
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
      routes: [],
      removeRoutes: () => {
        for (const routeId of mapActions.current?.routes || []) {
          map.removeLayer(routeId);
          map.removeSource(routeId);
        }
        if (mapActions.current) {
          mapActions.current.routes = [];
        }
      },
      addRoute: (id, route, color) => {
        map.addSource(id, {
          type: "geojson",
          data: {
            type: "LineString",
            coordinates: route.map((c) => [c[1], c[0]]),
          },
        });
        map.addLayer({
          id,
          type: "line",
          source: id,
          layout: {
            "line-cap": "round",
            "line-join": "round",
          },
          paint: {
            "line-color": color,
            "line-width": 5,
            "line-opacity": 0.8,
          },
        });
        mapActions.current?.routes.push(id);
      },
      removePoints: () => {
        for (const pointId of mapActions.current?.points || []) {
          map.removeLayer(pointId);
          map.removeSource(pointId);
          map.removeLayer(`${pointId}-radius`);
          map.removeSource(`${pointId}-radius`);
        }
        if (mapActions.current) {
          mapActions.current.points = [];
        }
      },
      removeMarkers: () => {
        for (const [_id, marker] of mapActions.current?.markers.entries() ||
          []) {
          marker.remove();
        }
        if (mapActions.current) {
          mapActions.current.markers = new Map();
        }
      },
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
        mapActions.current?.points.push(id);
      },
      addMarker: ({ id, lat, lon, markerName }) => {
        const marker = new maplibregl.Marker();
        marker.setLngLat([lon, lat]);
        const popup = new maplibregl.Popup();
        popup.setText(markerName);
        marker.setPopup(popup);
        marker.addTo(map);
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
  return div(Itineraries(), ItineraryWaypoints(), Steps(), MapContainer());
};

van.add(document.body, App());
