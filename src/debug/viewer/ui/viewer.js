/** @import * as maplibregl from "./maplibre-gl" */
import * as turf from "./turf.js";

import van from "./van-1.5.2.debug.js";

const { button, div, table, td, th, tr } = van.tags;
const selection = van.state(
  /** @type {import("./viewer.types.js").SelectionState} */ ({
    itinerary: null,
  }),
);
const itineraries = van.state(
  /** @type {import("./api-types.ts").DebugStreamItineraries[]} */ ([]),
);
const itineraryWaypoints = van.state(
  /** @type {import("./api-types.ts").DebugStreamItineraryWaypoints[]} */ ([]),
);
const steps = van.state(
  /** @type {import("./api-types.ts").DebugStreamSteps[]} */ ([]),
);
const stepResults = van.state(
  /** @type {import("./api-types.ts").DebugStreamStepResults[]} */ ([]),
);
const forkChoices = van.state(
  /** @type {import("./api-types.ts").DebugStreamForkChoices[]} */ ([]),
);
const forkChoiceWeights = van.state(
  /** @type {import("./api-types.ts").DebugStreamForkChoiceWeights[]} */ ([]),
);

/** @type {import("./viewer.types.js").MapActions} */
const mapActions = {
  markers: new Map(),
  addMarker: null,
  addPoint: null,
  points: [],
  setCenter: null,
};

/**
 * @param {number} page
 * @param {import("./viewer.types.js").PaginationClick} next
 * @param {import("./viewer.types.js").PaginationClick} back
 */
const Pagination = (page, next, back) => {
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
    for (const [_id, marker] of mapActions.markers.entries()) {
      marker.remove();
    }
    mapActions.markers = new Map();
    if (!selection.val.itinerary) {
      return;
    }
    if (!mapActions.addMarker) {
      alert("map not loaded");
      return;
    }
    if (!mapActions.setCenter) {
      alert("map not loaded");
      return;
    }
    const features = turf.points([
      [selection.val.itinerary.start_lon, selection.val.itinerary.start_lat],
      [selection.val.itinerary.finish_lon, selection.val.itinerary.finish_lat],
      ...itineraryWaypoints.val.map((wp) => [wp.lon, wp.lat]),
    ]);

    const center = turf.center(features);
    mapActions.setCenter(center);

    mapActions.addMarker({
      id: "start",
      markerName: "Start",
      lat: selection.val.itinerary.start_lat,
      lon: selection.val.itinerary.start_lon,
    });
    mapActions.addMarker({
      id: "finish",
      markerName: "Finish",
      lat: selection.val.itinerary.finish_lat,
      lon: selection.val.itinerary.finish_lon,
    });
    itineraryWaypoints.val.forEach((wp) => {
      if (!mapActions.addMarker) {
        alert("map not loaded");
        return;
      }
      mapActions.addMarker({
        id: `${wp.itinerary_id}-${wp.idx}`,
        markerName: `wp-${wp.idx}`,
        lat: wp.lat,
        lon: wp.lon,
      });
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
        tr([th("num"), th("itineraryId"), th("idx"), th("lat"), th("lon")]),
        ...itineraryWaypoints.val.map((it, idx) =>
          tr([
            td(page.val * pageSize + idx + 1),
            td(it.itinerary_id),
            button(
              {
                class: "dark:hover:bg-gray-800 hover:bg-gray-200",
                onclick: () => {
                  if (!mapActions.addMarker) {
                    alert("map not ready");
                  } else {
                    mapActions.addMarker({
                      id: `${it.itinerary_id}-${it.idx}`,
                      lat: it.lat,
                      lon: it.lon,
                      markerName: `wp-${it.idx}`,
                    });
                  }
                },
              },
              td(it.idx),
            ),
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
    mapActions.addPoint = ({ id, lat, lon, pointName, radius }) => {
      map.addSource(id, {
        type: "geojson",
        properties: {
          pointName,
        },
        data: {
          type: "Point",
          coordinates: [lon, lat],
        },
      });
      map.addLayer({
        id,
        source: id,
        type: "circle",
        layout: {
          "text-field": ["get", "description"],
          "text-variable-anchor": ["top", "bottom", "left", "right"],
          "text-radial-offset": 0.5,
          "text-justify": "auto",
        },
        paint: {
          "circle-radius": 5,
          "circle-color": "#007cbf",
        },
      });
      if (radius) {
        const options = {
          steps: 64,
          units: "meters",
        };
        const circle = turf.circle([lon, lat], radius, options);
        map.addSource(`${id}-radius`, {
          type: "geojson",
          data: {
            type: "polygon",
            data: circle,
          },
        });
        map.addLayer({
          id,
          type: "fill",
          source: id,
          paint: {
            "fill-color": "#8CCFFF",
            "fill-opacity": 0.5,
          },
        });
      }
    };
    mapActions.addMarker = ({ id, lat, lon, markerName }) => {
      const marker = new maplibregl.Marker();
      marker.setLngLat([lon, lat]).addTo(map);
      const popup = new maplibregl.Popup();
      popup.setText(markerName);
      marker.setPopup(popup);
      mapActions.markers.set(id, marker);
      return () => marker.remove();
    };
    mapActions.setCenter = (center) => {
      map.setCenter(center);
    };
  });

  return mapContainer;
};

const App = () => {
  return div(Itineraries(), ItineraryWaypoints(), MapContainer());
};

van.add(document.body, App());
