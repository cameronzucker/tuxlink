import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import { useAttachments } from './useAttachments';

beforeEach(() => invokeMock.mockReset());

describe('useAttachments', () => {
  it('adds a prepared attachment from a path via prepare_attachment', async () => {
    invokeMock.mockResolvedValue({
      filename: 'photo.jpg',
      bytes: [1, 2, 3],
      kind: 'image',
      originalLen: 2000000,
      newLen: 50000,
    });
    const { result } = renderHook(() => useAttachments());
    await act(async () => {
      await result.current.addPath('/tmp/photo.heic');
    });
    expect(invokeMock).toHaveBeenCalledWith(
      'prepare_attachment',
      expect.objectContaining({ path: '/tmp/photo.heic', imagePreset: 'medium', imageFormat: 'jpeg' }),
    );
    expect(result.current.items).toHaveLength(1);
    expect(result.current.items[0].filename).toBe('photo.jpg');
  });

  it('removes by index', async () => {
    invokeMock.mockResolvedValue({ filename: 'a.txt', bytes: [1], kind: 'file', originalLen: 1, newLen: 1 });
    const { result } = renderHook(() => useAttachments());
    await act(async () => {
      await result.current.addPath('/tmp/a.txt');
    });
    act(() => {
      result.current.remove(0);
    });
    expect(result.current.items).toHaveLength(0);
  });

  it('exposes the DTO shape message_send expects', async () => {
    invokeMock.mockResolvedValue({ filename: 'a.jpg', bytes: [9, 9], kind: 'image', originalLen: 100, newLen: 2 });
    const { result } = renderHook(() => useAttachments());
    await act(async () => {
      await result.current.addPath('/tmp/a.png');
    });
    expect(result.current.toDto()).toEqual([{ filename: 'a.jpg', bytes: [9, 9] }]);
  });

  it('surfaces an error when prepare_attachment rejects', async () => {
    // Pattern mirrors useAprsChat.test.ts: mockRejectedValueOnce + a no-op
    // .catch() at the call site so vitest's unhandled-rejection detector stays
    // quiet (addPath already swallows internally; the hook extracts .message).
    invokeMock.mockRejectedValueOnce(new Error('decode failed: garbage'));
    const { result } = renderHook(() => useAttachments());
    await act(async () => {
      await result.current.addPath('/tmp/bad.png').catch(() => {});
    });
    expect(result.current.items).toHaveLength(0);
    expect(result.current.error).toBe('decode failed: garbage');
  });

  it('re-transcodes an image in place when setOptions changes the preset', async () => {
    invokeMock
      .mockResolvedValueOnce({ filename: 'p.jpg', bytes: [1], kind: 'image', originalLen: 2_000_000, newLen: 90_000 })
      .mockResolvedValueOnce({ filename: 'p.jpg', bytes: [2], kind: 'image', originalLen: 2_000_000, newLen: 30_000 });
    const { result } = renderHook(() => useAttachments());
    await act(async () => {
      await result.current.addPath('/tmp/p.heic', { resize: 'medium', format: 'jpeg' });
    });
    expect(result.current.items[0].newLen).toBe(90_000);

    await act(async () => {
      await result.current.setOptions(0, { resize: 'small', format: 'jpeg' });
    });
    // Re-invoked with the retained source path + the new preset; item replaced.
    expect(invokeMock).toHaveBeenLastCalledWith(
      'prepare_attachment',
      expect.objectContaining({ path: '/tmp/p.heic', imagePreset: 'small' }),
    );
    expect(result.current.items).toHaveLength(1);
    expect(result.current.items[0].newLen).toBe(30_000);
    expect(result.current.items[0].opts.resize).toBe('small');
  });

  it('setOptions is a no-op for non-image files', async () => {
    invokeMock.mockResolvedValueOnce({ filename: 'a.txt', bytes: [1], kind: 'file', originalLen: 1, newLen: 1 });
    const { result } = renderHook(() => useAttachments());
    await act(async () => {
      await result.current.addPath('/tmp/a.txt');
    });
    invokeMock.mockClear();
    await act(async () => {
      await result.current.setOptions(0, { resize: 'small', format: 'webp' });
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
