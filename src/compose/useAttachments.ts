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

export interface ImageOpts {
  preset: 'small' | 'medium' | 'large' | 'original';
  format: 'jpeg' | 'webp';
}

const DEFAULT_OPTS: ImageOpts = { preset: 'medium', format: 'jpeg' };

export function useAttachments() {
  const [items, setItems] = useState<PreparedAttachment[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const addPath = useCallback(async (path: string, opts: ImageOpts = DEFAULT_OPTS) => {
    setBusy(true);
    setError(null);
    try {
      const prepared = await invoke<PreparedAttachment>('prepare_attachment', {
        path,
        imagePreset: opts.preset,
        imageFormat: opts.format,
      });
      setItems((prev) => [...prev, prepared]);
    } catch (e) {
      // Tauri commands reject with a String; be defensive about Error too.
      const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : 'Could not attach that file.';
      setError(msg);
    } finally {
      setBusy(false);
    }
  }, []);

  const remove = useCallback((index: number) => {
    setItems((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const totalBytes = items.reduce((sum, a) => sum + a.newLen, 0);

  const toDto = useCallback(
    (): AttachmentDto[] => items.map((a) => ({ filename: a.filename, bytes: a.bytes })),
    [items],
  );

  return { items, busy, error, addPath, remove, totalBytes, toDto };
}
