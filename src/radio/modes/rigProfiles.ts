// src/radio/modes/rigProfiles.ts
//
// Bundled, documented per-radio pre-fill profiles (tuxlink-31c63). Keyed by
// hamlib model id. This is OBJECTIVE PRODUCT DATA — documented known-good
// ARDOP/VARA settings for a radio (e.g. a radio whose internal codec resets
// when the CAT serial is held open during audio needs close-serial sequencing).
// It is NOT any operator's personal tuning, and carries no "popular"/preferred
// ranking. A model absent from this table simply gets no pre-fill.
//
// Criterion to add an entry: the value must be DOCUMENTED known-good for that
// radio's ARDOP/VARA operation (a hardware datasheet, a manufacturer note, or a
// reproduced-and-recorded on-air result). Do not guess.

/** Mirrors the backend `PttMethod` (config.rs / ArdopUiConfig.ptt_method). */
export type RigProfilePttMethod = 'vox' | 'serial_rts' | 'cat_command';

/** A per-radio pre-fill profile. Every field optional — only documented fields
 *  are present, and pre-fill skips any field the profile omits. */
export interface RigProfile {
  ptt_method?: RigProfilePttMethod;
  data_mode?: string;
  cat_baud?: number;
  close_serial_sequencing?: boolean;
}

/** model id → documented profile. */
export const RIG_PROFILES: Record<number, RigProfile> = {
  // Yaesu FT-710 (hamlib 1049): the internal SCU-LAN/codec resets if the CAT
  // serial is held open during audio, so it keys ONLY by CAT command and needs
  // close-serial sequencing. 38400 is the Enhanced-port default. Documented +
  // reproduced (project_ft710_internal_codec_tx_reset).
  1049: {
    ptt_method: 'cat_command',
    data_mode: 'PKTUSB',
    cat_baud: 38400,
    close_serial_sequencing: true,
  },
};

/** Look up a radio's profile by hamlib model id; undefined when unset or
 *  unprofiled. */
export function getRigProfile(modelId: number | null | undefined): RigProfile | undefined {
  if (modelId === null || modelId === undefined) return undefined;
  return RIG_PROFILES[modelId];
}
