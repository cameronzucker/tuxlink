import { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

/** Result of the backend `prepare_attachment` command (tuxlink-mg4s). */
export interface PreparedAttachment {
  filename: string;
  bytes: number[];
  kind: 'image' | 'file';
  originalLen: number;
  newLen: number;
}

/** The shape `message_send`'s draft.attachments expects (OutboundAttachmentDto). */
export interface AttachmentDto {
  filename: string;
  bytes: number[];
}

/** Two independent operator controls (tuxlink-rbhg): `resize` (dimensions) and
 * `format` (re-encode). `format: 'original'` keeps the source format — combined
 * with `resize: 'original'` it's a byte-for-byte passthrough of the source file. */
export interface ImageOpts {
  resize: 'original' | 'small' | 'medium' | 'large';
  format: 'original' | 'jpeg' | 'webp';
}

/** A list entry: the backend result plus the source `path` and current `opts`,
 * retained so an image can be RE-transcoded at a different preset/format
 * without re-picking the file (tuxlink-rbhg). */
export interface AttachmentItem extends PreparedAttachment {
  path: string;
  opts: ImageOpts;
}

const DEFAULT_OPTS: ImageOpts = { resize: 'medium', format: 'jpeg' };

export function useAttachments() {
  const [items, setItems] = useState<AttachmentItem[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const errMsg = (e: unknown): string =>
    typeof e === 'string' ? e : e instanceof Error ? e.message : 'Could not attach that file.';

  const addPath = useCallback(async (path: string, opts: ImageOpts = DEFAULT_OPTS) => {
    setBusy(true);
    setError(null);
    try {
      const prepared = await invoke<PreparedAttachment>('prepare_attachment', {
        path,
        imagePreset: opts.resize,
        imageFormat: opts.format,
      });
      setItems((prev) => [...prev, { ...prepared, path, opts }]);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  }, []);

  /** Re-transcode the image at `index` with new preset/format and replace it
   * in place (updates filename/bytes/newLen live). No-op for non-image files —
   * their bytes don't depend on the image options. */
  const setOptions = useCallback(
    async (index: number, opts: ImageOpts) => {
      const current = items[index];
      if (!current || current.kind !== 'image') return;
      setBusy(true);
      setError(null);
      try {
        const prepared = await invoke<PreparedAttachment>('prepare_attachment', {
          path: current.path,
          imagePreset: opts.resize,
          imageFormat: opts.format,
        });
        setItems((prev) => prev.map((it, i) => (i === index ? { ...prepared, path: current.path, opts } : it)));
      } catch (e) {
        setError(errMsg(e));
      } finally {
        setBusy(false);
      }
    },
    [items],
  );

  const remove = useCallback((index: number) => {
    setItems((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const totalBytes = items.reduce((sum, a) => sum + a.newLen, 0);

  const toDto = useCallback(
    (): AttachmentDto[] => items.map((a) => ({ filename: a.filename, bytes: a.bytes })),
    [items],
  );

  return { items, busy, error, addPath, setOptions, remove, totalBytes, toDto };
}
