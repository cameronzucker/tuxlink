import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { importPreview, importCommit, importCancel, openFormsFolder, formsCustomDelete } from './importApi';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('importApi', () => {
  beforeEach(() => vi.clearAllMocks());

  it('importPreview passes sources and returns the plan', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      stagingToken: 'abc',
      entries: [],
      summary: {},
    });
    const plan = await importPreview(['/x/org.zip']);
    expect(invoke).toHaveBeenCalledWith('forms_import_preview', { sources: ['/x/org.zip'] });
    expect(plan.stagingToken).toBe('abc');
  });

  it('importCommit passes token + approved ids in camelCase', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      installed: [],
      skippedUpdates: [],
      entries: [],
    });
    await importCommit('abc', ['Foo Initial']);
    expect(invoke).toHaveBeenCalledWith('forms_import_commit', {
      stagingToken: 'abc',
      approvedOverwriteIds: ['Foo Initial'],
    });
  });

  it('importCancel resolves without throwing on a bad token', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    await expect(importCancel('abc')).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith('forms_import_cancel', { stagingToken: 'abc' });
  });

  it('openFormsFolder invokes the reveal command', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    await openFormsFolder();
    expect(invoke).toHaveBeenCalledWith('open_forms_folder');
  });

  it('formsCustomDelete passes ids and returns removed list', async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockResolvedValue(['Foo Initial']);
    const removed = await formsCustomDelete(['Foo Initial']);
    expect(invoke).toHaveBeenCalledWith('forms_custom_delete', { ids: ['Foo Initial'] });
    expect(removed).toEqual(['Foo Initial']);
  });
});
