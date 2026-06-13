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
});
