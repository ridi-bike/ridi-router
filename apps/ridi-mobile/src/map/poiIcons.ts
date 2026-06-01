import type { Expression } from '@maplibre/maplibre-react-native';

export const POI_ICON_IMAGES = {
  'poi-airport': { source: require('@/assets/map/poi-icons/airport.png'), sdf: true },
  'poi-cafe': { source: require('@/assets/map/poi-icons/cafe.png'), sdf: true },
  'poi-restaurant': { source: require('@/assets/map/poi-icons/restaurant.png'), sdf: true },
  'poi-bar': { source: require('@/assets/map/poi-icons/bar.png'), sdf: true },
  'poi-beer': { source: require('@/assets/map/poi-icons/beer.png'), sdf: true },
  'poi-alcohol-shop': { source: require('@/assets/map/poi-icons/alcohol-shop.png'), sdf: true },
  'poi-grocery': { source: require('@/assets/map/poi-icons/grocery.png'), sdf: true },
  'poi-shop': { source: require('@/assets/map/poi-icons/shop.png'), sdf: true },
  'poi-lodging': { source: require('@/assets/map/poi-icons/lodging.png'), sdf: true },
  'poi-campsite': { source: require('@/assets/map/poi-icons/campsite.png'), sdf: true },
  'poi-park': { source: require('@/assets/map/poi-icons/park.png'), sdf: true },
  'poi-amusement': { source: require('@/assets/map/poi-icons/amusement-park.png'), sdf: true },
  'poi-attraction': { source: require('@/assets/map/poi-icons/attraction.png'), sdf: true },
  'poi-museum': { source: require('@/assets/map/poi-icons/museum.png'), sdf: true },
  'poi-monument': { source: require('@/assets/map/poi-icons/monument.png'), sdf: true },
  'poi-castle': { source: require('@/assets/map/poi-icons/castle.png'), sdf: true },
  'poi-historic': { source: require('@/assets/map/poi-icons/historic.png'), sdf: true },
  'poi-viewpoint': { source: require('@/assets/map/poi-icons/viewpoint.png'), sdf: true },
  'poi-parking': { source: require('@/assets/map/poi-icons/parking.png'), sdf: true },
  'poi-fuel': { source: require('@/assets/map/poi-icons/fuel.png'), sdf: true },
  'poi-car': { source: require('@/assets/map/poi-icons/car.png'), sdf: true },
  'poi-bicycle': { source: require('@/assets/map/poi-icons/bicycle.png'), sdf: true },
  'poi-bus': { source: require('@/assets/map/poi-icons/bus.png'), sdf: true },
  'poi-rail': { source: require('@/assets/map/poi-icons/rail.png'), sdf: true },
  'poi-hospital': { source: require('@/assets/map/poi-icons/hospital.png'), sdf: true },
  'poi-doctor': { source: require('@/assets/map/poi-icons/doctor.png'), sdf: true },
  'poi-pharmacy': { source: require('@/assets/map/poi-icons/pharmacy.png'), sdf: true },
  'poi-school': { source: require('@/assets/map/poi-icons/school.png'), sdf: true },
  'poi-college': { source: require('@/assets/map/poi-icons/college.png'), sdf: true },
  'poi-library': { source: require('@/assets/map/poi-icons/library.png'), sdf: true },
  'poi-toilet': { source: require('@/assets/map/poi-icons/toilet.png'), sdf: true },
  'poi-drinking-water': { source: require('@/assets/map/poi-icons/drinking-water.png'), sdf: true },
  'poi-information': { source: require('@/assets/map/poi-icons/information.png'), sdf: true },
  'poi-post': { source: require('@/assets/map/poi-icons/post.png'), sdf: true },
  'poi-town-hall': { source: require('@/assets/map/poi-icons/town-hall.png'), sdf: true },
  'poi-recycling': { source: require('@/assets/map/poi-icons/recycling.png'), sdf: true },
  'poi-picnic': { source: require('@/assets/map/poi-icons/picnic-site.png'), sdf: true },
  'poi-playground': { source: require('@/assets/map/poi-icons/playground.png'), sdf: true },
  'poi-swimming': { source: require('@/assets/map/poi-icons/swimming.png'), sdf: true },
  'poi-golf': { source: require('@/assets/map/poi-icons/golf.png'), sdf: true },
  'poi-religious': { source: require('@/assets/map/poi-icons/place-of-worship.png'), sdf: true },
  'poi-marker': { source: require('@/assets/map/poi-icons/marker.png'), sdf: true },
} as const;

const FOOD_AND_DRINK = [
  'restaurant', 'cafe', 'bakery', 'bbq', 'butcher', 'greengrocer', 'grocery', 'supermarket', 'convenience',
] as const;

const BARS_AND_BEVERAGES = [
  'bar', 'pub', 'biergarten', 'beer', 'beverages', 'alcohol', 'alcohol_shop', 'wine', 'winery', 'brewery', 'nightclub',
] as const;

const SHOPS = [
  'adult_gaming_centre', 'atm', 'beauty', 'bookmaker', 'books', 'bureau_de_change', 'car_parts', 'clothes',
  'computer', 'fashion', 'florist', 'gambling', 'garden_centre', 'gift', 'hairdresser', 'hifi', 'jewelry',
  'karaoke', 'karaoke_box', 'lottery', 'mobile_phone', 'money_transfer', 'newsagent', 'optician', 'perfumery',
  'ship_chandler', 'stationery', 'tobacco', 'travel_agency',
] as const;

const LODGING = ['hotel', 'hostel', 'guest_house', 'bed_and_breakfast', 'chalet', 'alpine_hut', 'wilderness_hut'] as const;
const OUTDOORS = ['beach', 'forest', 'garden', 'national_park', 'nature_reserve', 'park', 'peak'] as const;
const AMUSEMENT = ['amusement_ride', 'carousel', 'roller_coaster', 'summer_toboggan', 'water_slide'] as const;
const ATTRACTIONS = ['animal', 'attraction', 'building', 'hanami', 'landmark', 'maze', 'zoo'] as const;
const CULTURE = ['art', 'artwork', 'studio'] as const;
const HERITAGE = ['castle', 'manor', 'mansion', 'memorial', 'ruins', 'wayside_cross'] as const;
const TRANSPORT = ['aerodrome', 'airfield', 'bus_stop', 'harbourmaster', 'marina', 'slipway', 'station', 'taxi'] as const;
const CAR = ['atv', 'boat_storage', 'car', 'car_rental', 'car_repair', 'car_sharing', 'car_wash', 'motorcycle', 'snowmobile'] as const;
const PARKING = ['bicycle_parking', 'motorcycle_parking', 'parking'] as const;
const HEALTH = ['baby_hatch', 'clinic', 'emergency_phone', 'hospital'] as const;
const EDUCATION = ['childcare', 'kindergarten', 'school'] as const;
const UTILITIES = ['customs', 'district', 'fire_station', 'military', 'naval_base', 'social_facility', 'telephone', 'townhall'] as const;
const WASTE = ['recycling', 'waste_basket', 'waste_disposal'] as const;
const WATER_DRINKING = ['drinking_water', 'water_point', 'watering_place'] as const;
const WATER_SANITATION = ['sanitary_dump_station', 'shower', 'toilets'] as const;
const PICNIC = ['bench', 'firepit', 'picnic_site', 'picnic_table', 'shelter'] as const;
const SPORTS = ['fishing', 'hunting_stand', 'pitch', 'playground', 'scuba_diving', 'stadium', 'swimming_area', 'trail_riding_station'] as const;
const WORSHIP = ['cemetery', 'church', 'mosque', 'place_of_worship', 'religion', 'religious', 'shrine', 'synagogue', 'temple'] as const;

export const POI_ICON_IMAGE_EXPRESSION = [
  'match',
  ['get', 'kind'],
  ['aerodrome', 'airfield'], 'poi-airport',
  ['restaurant'], 'poi-restaurant',
  ['cafe', 'bakery', 'bbq'], 'poi-cafe',
  ['beverages', 'alcohol', 'alcohol_shop', 'wine', 'winery', 'brewery'], 'poi-beer',
  BARS_AND_BEVERAGES, 'poi-bar',
  ['grocery', 'greengrocer', 'supermarket', 'convenience', 'butcher'], 'poi-grocery',
  SHOPS, 'poi-shop',
  LODGING, 'poi-lodging',
  ['camp_site'], 'poi-campsite',
  OUTDOORS, 'poi-park',
  AMUSEMENT, 'poi-amusement',
  ATTRACTIONS, 'poi-attraction',
  CULTURE, 'poi-museum',
  HERITAGE, 'poi-castle',
  ['historic'], 'poi-historic',
  ['viewpoint'], 'poi-viewpoint',
  PARKING, 'poi-parking',
  ['fuel', 'charging_station'], 'poi-fuel',
  CAR, 'poi-car',
  ['bicycle_rental', 'bicycle_repair_station'], 'poi-bicycle',
  ['bus_stop', 'taxi'], 'poi-bus',
  ['station'], 'poi-rail',
  HEALTH, 'poi-hospital',
  ['doctors', 'dentist'], 'poi-doctor',
  ['pharmacy'], 'poi-pharmacy',
  EDUCATION, 'poi-school',
  ['college', 'university'], 'poi-college',
  ['library'], 'poi-library',
  WATER_DRINKING, 'poi-drinking-water',
  WATER_SANITATION, 'poi-toilet',
  ['information', 'ranger_station', 'emergency_phone', 'life_ring'], 'poi-information',
  ['post_box', 'post_office'], 'poi-post',
  UTILITIES, 'poi-town-hall',
  WASTE, 'poi-recycling',
  PICNIC, 'poi-picnic',
  ['playground', 'dog_park'], 'poi-playground',
  ['swimming_area', 'scuba_diving'], 'poi-swimming',
  ['golf', 'golf_course'], 'poi-golf',
  WORSHIP, 'poi-religious',
  'poi-marker',
] as unknown as Expression;

export const POI_ICON_COLOR_EXPRESSION = [
  'match',
  ['get', 'kind'],
  FOOD_AND_DRINK, '#D97706',
  BARS_AND_BEVERAGES, '#B45309',
  SHOPS, '#C026D3',
  LODGING, '#7C3AED',
  ['camp_site'], '#4F46E5',
  [...OUTDOORS, ...PICNIC], '#16A34A',
  [...AMUSEMENT, ...ATTRACTIONS, ...CULTURE, 'viewpoint'], '#DB2777',
  [...HERITAGE, 'historic'], '#92400E',
  [...TRANSPORT, ...CAR, ...PARKING, 'fuel', 'charging_station', 'bicycle_rental', 'bicycle_repair_station'], '#2563EB',
  [...HEALTH, 'doctors', 'dentist', 'pharmacy'], '#DC2626',
  [...EDUCATION, 'college', 'university', 'library'], '#0891B2',
  [...WATER_DRINKING, ...WATER_SANITATION, 'swimming_area', 'scuba_diving'], '#0284C7',
  [...UTILITIES, ...WASTE, 'information', 'ranger_station', 'post_box', 'post_office', 'life_ring'], '#475569',
  [...SPORTS, 'dog_park', 'golf', 'golf_course'], '#65A30D',
  WORSHIP, '#64748B',
  '#9333EA',
] as unknown as Expression;
