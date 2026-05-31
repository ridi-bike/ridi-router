export type RidiRouterErrorCode =
  | 'invalid_request_json'
  | 'invalid_tiles_dir'
  | 'routing_open_failed'
  | 'route_generation_failed'
  | 'response_serialization_failed'
  | 'native_binding_failed'
  | 'unknown';

export type RidiRouterError = {
  code: RidiRouterErrorCode;
  message: string;
  details?: string;
};

export type RidiRouterResult<T> =
  | { ok: true; value: T }
  | { ok: false; error: RidiRouterError };
