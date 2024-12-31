import van from "./van-1.5.2.debug.js";

const { button, div, table, td, th, tr } = van.tags;

const Itineraries = () => {
  const itineraries = van.state(
    /** @type {import("./api-types.ts").DebugStreamItineraries[]} */ ([]),
  );

  fetch("http://0.0.0.0:1337/data/DebugStreamItineraries?limit=20&offset=0")
    .then((req) => req.json())
    .then((data) => (itineraries.val = data))
    .catch(console.error);

  van.derive(() => console.log(itineraries.val));

  return div(() =>
    table([
      tr([th("id"), th("wp_"), th("radius"), th("visit_all")]),
      ...itineraries.val.map((it) =>
        tr([
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

  return mapContainer;
};

const App = () => {
  return div(Itineraries(), MapContainer());
};

van.add(document.body, App());
