// weightsApi.ts — the single invoke/event surface for the classifier-weights
// job (tuxlink-13ofm), mirroring the offlineMaps.ts convention: every
// component goes through these wrappers, no raw invoke() calls elsewhere.

import { invoke } from '@tauri-apps/api/core';

/** Mirror of the Rust `JobDto` (serde camelCase). */
export interface WeightsJob {
  state: 'waiting' | 'downloading' | 'verifying' | 'complete' | 'failed';
  /** Waiting reason / failure message, operator-phrased. */
  detail: string | null;
  errorClass: 'network' | 'source' | 'digest-mismatch' | 'io' | 'cancelled' | null;
  /** File currently moving, when downloading/verifying. */
  file: string | null;
  filesDone: string[];
  /** Where the bytes come from, as a display string. */
  source: string;
  startedUnix: number;
  updatedUnix: number;
}

/** Mirror of the Rust `WeightsStatusDto`. */
export interface WeightsStatus {
  modelId: string;
  totalBytes: number;
  ready: boolean;
  integrity: 'digest-pinned' | 'size-verified' | 'structure' | null;
  location: string | null;
  summary: string;
  defaultSource: string;
  job: WeightsJob | null;
}

/** Mirror of the Rust `ProgressPayload`. */
export interface WeightsProgress {
  file: string;
  got: number;
  total: number;
}

/** Throttled byte progress while a file streams or verifies. */
export const WEIGHTS_PROGRESS_EVENT = 'classify-weights:progress';
/** Full status DTO on every phase change. */
export const WEIGHTS_STATUS_EVENT = 'classify-weights:status';

export function weightsStatus(): Promise<WeightsStatus> {
  return invoke<WeightsStatus>('classify_weights_status');
}

/** Start (or retry) the download; empty/undefined source uses the
 * version-matched release default. */
export function weightsDownloadStart(sourceUrl?: string): Promise<WeightsStatus> {
  return invoke<WeightsStatus>('classify_weights_download_start', {
    sourceUrl: sourceUrl && sourceUrl.trim() !== '' ? sourceUrl.trim() : null,
  });
}

export function weightsDownloadCancel(): Promise<WeightsStatus> {
  return invoke<WeightsStatus>('classify_weights_download_cancel');
}

/** Install from a local folder holding the three files — verified against the
 * same release pins as the download. */
export function weightsSideloadImport(dir: string): Promise<WeightsStatus> {
  return invoke<WeightsStatus>('classify_weights_sideload_import', { dir });
}

/** `134178443 → "128 MB"` — coarse, for buttons and progress lines. */
export function formatMb(bytes: number): string {
  return `${Math.max(1, Math.round(bytes / (1024 * 1024)))} MB`;
}
