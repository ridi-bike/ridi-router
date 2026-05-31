import { requireNativeModule } from 'expo-modules-core';

import type { RidiRouterResult } from './RidiRouter.types';

export type { RidiRouterError, RidiRouterErrorCode, RidiRouterResult } from './RidiRouter.types';

type RidiRouterNativeModule = {
  generateRouteJson(tilesDir: string, requestJson: string): Promise<string>;
};

const RidiRouter = requireNativeModule<RidiRouterNativeModule>('RidiRouter');

const nativeBindingFailed = (error: unknown): RidiRouterResult<unknown> => ({
  ok: false,
  error: {
    code: 'native_binding_failed',
    message: 'Native routing call failed',
    details: String(error),
  },
});

const invalidNativeEnvelope = (details: string): RidiRouterResult<unknown> => ({
  ok: false,
  error: {
    code: 'native_binding_failed',
    message: 'Native routing call returned an invalid envelope',
    details,
  },
});

const isEnvelope = (value: unknown): value is RidiRouterResult<unknown> => {
  if (typeof value !== 'object' || value === null || !('ok' in value)) {
    return false;
  }

  const envelope = value as { ok?: unknown; value?: unknown; error?: unknown };
  if (envelope.ok === true) {
    return 'value' in envelope;
  }

  if (envelope.ok === false) {
    const error = envelope.error;
    return typeof error === 'object' && error !== null && 'code' in error && 'message' in error;
  }

  return false;
};

export async function generateRouteJson(
  tilesDir: string,
  requestJson: string,
): Promise<RidiRouterResult<unknown>> {
  try {
    const json = await RidiRouter.generateRouteJson(tilesDir, requestJson);
    const parsed = JSON.parse(json) as unknown;

    if (!isEnvelope(parsed)) {
      return invalidNativeEnvelope(json);
    }

    return parsed;
  } catch (error) {
    return nativeBindingFailed(error);
  }
}
