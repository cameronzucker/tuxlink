/** tuxlink-mg4s: pure helpers for the compose attachment UI. */

const IMAGE_EXTS = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'tif', 'tiff', 'bmp', 'heic', 'heif'];

export function isImageFilename(name: string): boolean {
  const dot = name.lastIndexOf('.');
  if (dot < 0) return false;
  return IMAGE_EXTS.includes(name.slice(dot + 1).toLowerCase());
}

export function humanSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Worst-case airtime at a ~90 B/s slow-packet floor — the figure that makes
 * the cost legible to the operator before they send. */
export function airtimeEstimate(bytes: number): string {
  const seconds = Math.round(bytes / 90);
  if (seconds < 90) return `~${seconds} sec on slow packet`;
  return `~${Math.round(seconds / 60)} min on slow packet`;
}

/** Winlink CMS message-size ceiling (~120 KB total, body + attachments).
 * Confirmed via ui_commands.rs:239 + the Hamexandria winlink-annex operator
 * threads (tuxlink-rbhg). A message over this is rejected by the CMS. */
export const CMS_LIMIT_BYTES = 120 * 1024;

export type CmsStatus = 'ok' | 'near' | 'over';

/** Classify a total message size against the CMS limit so the compose UI can
 * warn the operator before a send the CMS would reject. `near` fires within
 * 80% so there's headroom for the body text + B2F envelope on top of the
 * attachment bytes. */
export function cmsStatus(totalBytes: number): CmsStatus {
  if (totalBytes > CMS_LIMIT_BYTES) return 'over';
  if (totalBytes > CMS_LIMIT_BYTES * 0.8) return 'near';
  return 'ok';
}
