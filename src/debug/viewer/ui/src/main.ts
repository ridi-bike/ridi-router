import "./style.css";
import "maplibre-gl/dist/maplibre-gl.css";

import * as maplibregl from "maplibre-gl";
import * as turf from "@turf/turf";

import van from "vanjs-core";
import {
  DebugStreamForkChoices,
  DebugStreamItineraries,
  DebugStreamItineraryWaypoints,
  DebugStreamStepResults,
  DebugStreamSteps,
} from "./api-types";
import { MapActions, SelectionState } from "./types";
import {
  tableClass,
  tdClass,
  theadClass,
  thClass,
  trClass,
} from "./data-table";

const { button, thead, tbody, div, span, table, td, th, tr } = van.tags;
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
      table({ class: tableClass() }, [
        thead(
          { class: theadClass() },
          tr([
            th({ class: thClass() }, "num"),
            th({ class: thClass() }, "id"),
            th({ class: thClass() }, "wps"),
            th({ class: thClass() }, "radius"),
            th({ class: thClass() }, "visit_all"),
            th({ class: thClass() }, "start"),
            th({ class: thClass() }, "finish"),
          ]),
        ),
        tbody(
          ...itineraries.val.map((it, idx) =>
            tr(
              {
                class: () =>
                  trClass({
                    "bg-red-100":
                      selection.val.itinerary?.itinerary_id === it.itinerary_id,
                    "dark:bg-red-900":
                      selection.val.itinerary?.itinerary_id === it.itinerary_id,
                  }),
              },
              [
                td({ class: tdClass() }, page.val * pageSize + idx + 1),
                td(
                  { class: tdClass() },
                  button(
                    {
                      class: "dark:hover:bg-yellow-800 hover:bg-yellow-200",
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
                td({ class: tdClass() }, it.waypoints_count),
                td({ class: tdClass() }, it.radius),
                td({ class: tdClass() }, it.visit_all),
                td({ class: tdClass() }, `${it.start_lat},${it.start_lon}`),
                td({ class: tdClass() }, `${it.finish_lat},${it.finish_lon}`),
              ],
            ),
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
      table(
        { class: tableClass() },
        thead({ class: theadClass() }, [
          tr([
            th({ class: thClass() }, "num"),
            th({ class: thClass() }, "idx"),
            th({ class: thClass() }, "lat"),
            th({ class: thClass() }, "lon"),
          ]),
          tbody(
            ...itineraryWaypoints.val.map((it, idx) =>
              tr({ class: trClass() }, [
                td({ class: tdClass() }, page.val * pageSize + idx + 1),
                td({ class: tdClass() }, it.idx),
                td({ class: tdClass() }, it.lat),
                td({ class: tdClass() }, it.lon),
              ]),
            ),
          ),
        ]),
      ),
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
      table(
        { class: tableClass() },
        thead(
          { class: theadClass() },
          tr([
            th({ class: thClass() }, "step_num"),
            th({ class: thClass() }, "move_result"),
          ]),
        ),
        tbody(
          ...steps.val.map((step) => [
            tr(
              {
                class: () =>
                  trClass("font-bold", {
                    "bg-red-100":
                      selection.val.step?.step_num === step.step_num,
                    "dark:bg-red-900":
                      selection.val.step?.step_num === step.step_num,
                  }),
              },
              [
                td({ class: tdClass() }, step.step_num),
                td(
                  { class: tdClass() },
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
            tr(
              { class: trClass() },
              td({ class: tdClass() }, "Choices:"),
              td(
                { class: tdClass() },
                ForkChoices(step.itinerary_id, step.step_num),
              ),
            ),
            tr(
              { class: trClass() },
              td({ class: tdClass() }, "Step Result:"),
              td(
                { class: tdClass() },
                StepResult(step.itinerary_id, step.step_num),
              ),
            ),
          ]),
        ),
      ),
  );
};

const ForkChoices = (itineraryId: string, stepNum: number) => {
  const forkCHoices = van.state<DebugStreamForkChoices[]>([]);
  fetch(
    `http://0.0.0.0:1337/data/DebugStreamForkChoices?itinerary_id=${itineraryId}&step_num=${stepNum}`,
  )
    .then((resp) => resp.json())
    .then((data) => (forkCHoices.val = data));
  return div(() =>
    table(
      { class: tableClass() },
      thead(
        { class: theadClass() },
        tr(
          th({ class: thClass() }, "discarded"),
          th({ class: thClass() }, "end point id"),
          th({ class: thClass() }, "segment end point num"),
          th({ class: thClass() }, "point 0"),
          th({ class: thClass() }, "point 1"),
        ),
      ),
      tbody(
        ...forkCHoices.val.map((forkCh) =>
          tr({ class: trClass() }, [
            td({ class: tdClass() }, forkCh.discarded),
            td({ class: tdClass() }, forkCh.end_point_id),
            td({ class: tdClass() }, forkCh.segment_end_point),
            td(
              { class: tdClass() },
              `${forkCh.line_point_0_lat},${forkCh.line_point_0_lon}`,
            ),
            td(
              { class: tdClass() },
              `${forkCh.line_point_1_lat}, ${forkCh.line_point_1_lon}`,
            ),
          ]),
        ),
      ),
    ),
  );
};
const StepResult = (itineraryId: string, stepNum: number) => {
  const stepResults = van.state<DebugStreamStepResults[]>([]);
  fetch(
    `http://0.0.0.0:1337/data/DebugStreamStepResults?itinerary_id=${itineraryId}&step_num=${stepNum}`,
  )
    .then((resp) => resp.json())
    .then((data) => (stepResults.val = data));
  return div({ class: "pl-4" }, () =>
    table(
      { class: tableClass() },
      thead(
        { class: theadClass() },
        tr(
          th({ class: theadClass() }, "result"),
          th({ class: theadClass() }, "chosen fork point id"),
        ),
      ),
      tbody(
        ...stepResults.val.map((stepRes) =>
          tr({ class: trClass() }, [
            td({ class: tdClass() }, stepRes.result),
            td({ class: tdClass() }, stepRes.chosen_fork_point_id),
          ]),
        ),
      ),
    ),
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
