/** @import * from "./maplibre-gl.d.ts" */
import van from "./van-1.5.2.debug.js";

const { button, div, table, td, th, tr } = van.tags;

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

  van.derive(() => console.log(itineraries.val));

  return div(
    div(
      { class: "flex flex-row" },
      button(
        {
          class: "dark:hover:bg-gray-800 hover-gray-200 px-4 py-1",
          onclick: () => (page.val > 0 ? page.val-- : void 0),
        },
        "<",
      ),
      div({ class: "px-4 py-1" }, page.val + 1),
      button(
        {
          class: "dark:hover:bg-gray-800 hover-gray-200 px-4 py-1",
          onclick: () => page.val++,
        },
        ">",
      ),
    ),
    () =>
      table([
        tr([th("num"), th("id"), th("wp_"), th("radius"), th("visit_all")]),
        ...itineraries.val.map((it, idx) =>
          tr([
            td(page.val * pageSize + idx + 1),
            td(it.itinerary_id),
            td(it.waypoints_count),
            td(it.radius),
            td(it.visit_all),
          ]),
        ),
      ]),
  );
};

const MapContainer = () => {
  const mapContainer = div({ id: "map-container", class: "w-64 h-64" });

  var map = new maplibregl.Map({
    container: mapContainer,
    style: "https://demotiles.maplibre.org/style.json", // style URL
    center: [0, 0], // starting position [lng, lat]
    zoom: 1, // starting zoom
  });

  map.on("load", () => {
    map.addSource("point", {
      type: "geojson",
      data: {
        type: "Point",
        coordinates: [1, 2],
      },
    });
    map.addLayer({
      id: "point",
      source: "point",
      type: "circle",
      paint: {
        "circle-radius": 10,
        "circle-color": "#007cbf",
      },
    });
  });

  return mapContainer;
};

const App = () => {
  return div(Itineraries(), MapContainer());
};

van.add(document.body, App());
