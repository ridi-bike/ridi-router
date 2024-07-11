use serde_derive::Deserialize;
use serde_derive::Serialize;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsmData {
    pub version: f64,
    pub generator: String,
    pub osm3s: Osm3s,
    pub elements: Vec<Element>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Osm3s {
    #[serde(rename = "timestamp_osm_base")]
    pub timestamp_osm_base: String,
    pub copyright: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    #[serde(rename = "type")]
    pub type_field: String,
    pub id: u64,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub tags: Option<Tags>,
    #[serde(default)]
    pub nodes: Vec<u64>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tags {
    pub highway: Option<String>,
    pub sport: Option<String>,
    pub name: Option<String>,
    pub foot: Option<String>,
    pub lanes: Option<String>,
    pub lit: Option<String>,
    pub maxspeed: Option<String>,
    #[serde(rename = "ref")]
    pub ref_field: Option<String>,
    pub surface: Option<String>,
    pub oneway: Option<String>,
    pub embankment: Option<String>,
    pub hgv: Option<String>,
    pub smoothness: Option<String>,
    pub construction: Option<String>,
    #[serde(rename = "int_ref")]
    pub int_ref: Option<String>,
    pub shoulder: Option<String>,
    pub bridge: Option<String>,
    pub layer: Option<String>,
    pub area: Option<String>,
    pub tracktype: Option<String>,
    pub width: Option<String>,
    pub access: Option<String>,
    pub junction: Option<String>,
    pub note: Option<String>,
    pub source: Option<String>,
    #[serde(rename = "maxspeed:backward")]
    pub maxspeed_backward: Option<String>,
    #[serde(rename = "maxspeed:forward")]
    pub maxspeed_forward: Option<String>,
    pub sidewalk: Option<String>,
    pub bus: Option<String>,
    #[serde(rename = "public_transport")]
    pub public_transport: Option<String>,
    pub maxweight: Option<String>,
    #[serde(rename = "surface:note")]
    pub surface_note: Option<String>,
    pub bicycle: Option<String>,
    pub horse: Option<String>,
    #[serde(rename = "motor_vehicle")]
    pub motor_vehicle: Option<String>,
    #[serde(rename = "flood_prone")]
    pub flood_prone: Option<String>,
    #[serde(rename = "lane_markings")]
    pub lane_markings: Option<String>,
    pub noname: Option<String>,
    pub cycleway: Option<String>,
    pub proposed: Option<String>,
    #[serde(rename = "lanes:backward")]
    pub lanes_backward: Option<String>,
    #[serde(rename = "lanes:forward")]
    pub lanes_forward: Option<String>,
    #[serde(rename = "turn:lanes:forward")]
    pub turn_lanes_forward: Option<String>,
    #[serde(rename = "turn:lanes:backward")]
    pub turn_lanes_backward: Option<String>,
    #[serde(rename = "turn:lanes")]
    pub turn_lanes: Option<String>,
    #[serde(rename = "roller_ski")]
    pub roller_ski: Option<String>,
    #[serde(rename = "maxspeed:type")]
    pub maxspeed_type: Option<String>,
    #[serde(rename = "sidewalk:left")]
    pub sidewalk_left: Option<String>,
    #[serde(rename = "sidewalk:right")]
    pub sidewalk_right: Option<String>,
    pub cyclestreet: Option<String>,
    #[serde(rename = "sidewalk:right:bicycle")]
    pub sidewalk_right_bicycle: Option<String>,
    pub operator: Option<String>,
    #[serde(rename = "operator:abbr")]
    pub operator_abbr: Option<String>,
    #[serde(rename = "operator:website")]
    pub operator_website: Option<String>,
    #[serde(rename = "operator:wikipedia")]
    pub operator_wikipedia: Option<String>,
    pub distance: Option<String>,
    #[serde(rename = "name:en")]
    pub name_en: Option<String>,
    #[serde(rename = "name:lv")]
    pub name_lv: Option<String>,
    #[serde(rename = "name:ru")]
    pub name_ru: Option<String>,
    pub amenity: Option<String>,
    #[serde(rename = "drinking_water")]
    pub drinking_water: Option<String>,
    pub fee: Option<String>,
    pub parking: Option<String>,
    pub toilets: Option<String>,
    #[serde(rename = "name:etymology")]
    pub name_etymology: Option<String>,
    #[serde(rename = "name:etymology:wikidata")]
    pub name_etymology_wikidata: Option<String>,
    #[serde(rename = "sidewalk:both")]
    pub sidewalk_both: Option<String>,
    #[serde(rename = "cycleway:left")]
    pub cycleway_left: Option<String>,
    #[serde(rename = "cycleway:right")]
    pub cycleway_right: Option<String>,
    #[serde(rename = "cycleway:right:surface")]
    pub cycleway_right_surface: Option<String>,
    #[serde(rename = "parking:lane:right")]
    pub parking_lane_right: Option<String>,
    #[serde(rename = "sidewalk:right:surface")]
    pub sidewalk_right_surface: Option<String>,
    #[serde(rename = "sidewalk:left:surface")]
    pub sidewalk_left_surface: Option<String>,
    pub leisure: Option<String>,
    #[serde(rename = "trail_visibility")]
    pub trail_visibility: Option<String>,
    pub maxheight: Option<String>,
    pub wheelchair: Option<String>,
    pub wikidata: Option<String>,
    #[serde(rename = "maxweight:signed")]
    pub maxweight_signed: Option<String>,
    pub barrier: Option<String>,
    pub crossing: Option<String>,
    #[serde(rename = "crossing:markings")]
    pub crossing_markings: Option<String>,
    #[serde(rename = "traffic_sign")]
    pub traffic_sign: Option<String>,
    #[serde(rename = "traffic_sign:direction")]
    pub traffic_sign_direction: Option<String>,
    pub ford: Option<String>,
    #[serde(rename = "crossing:island")]
    pub crossing_island: Option<String>,
    #[serde(rename = "tactile_paving")]
    pub tactile_paving: Option<String>,
    pub noexit: Option<String>,
    pub direction: Option<String>,
    #[serde(rename = "traffic_calming")]
    pub traffic_calming: Option<String>,
    pub kerb: Option<String>,
    #[serde(rename = "crossing_ref")]
    pub crossing_ref: Option<String>,
    pub service: Option<String>,
    pub fixme: Option<String>,
    #[serde(rename = "crossing:barrier")]
    pub crossing_barrier: Option<String>,
    pub railway: Option<String>,
    #[serde(rename = "crossing:light")]
    pub crossing_light: Option<String>,
    #[serde(rename = "button_operated")]
    pub button_operated: Option<String>,
    #[serde(rename = "traffic_signals:countdown")]
    pub traffic_signals_countdown: Option<String>,
    #[serde(rename = "traffic_signals:sound")]
    pub traffic_signals_sound: Option<String>,
    #[serde(rename = "traffic_signals:vibration")]
    pub traffic_signals_vibration: Option<String>,
    pub entrance: Option<String>,
    #[serde(rename = "check_date:crossing")]
    pub check_date_crossing: Option<String>,
    #[serde(rename = "traffic_island")]
    pub traffic_island: Option<String>,
}
