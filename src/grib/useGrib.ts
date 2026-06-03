// useGrib — thin wrapper around the grib_send_request Tauri command.
// bd issue: tuxlink-vrpk.

import { invoke } from '@tauri-apps/api/core';
import type { GribRequest } from './types';

/// Compose + queue a Saildocs GRIB request. Returns the MID string on
/// success (mirrors message_send / catalog_send_inquiry contract).
export async function sendGribRequest(request: GribRequest): Promise<string> {
  return invoke<string>('grib_send_request', { request });
}
