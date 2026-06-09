import { describe, it, expect } from 'vitest';
import { folderBearsReadState } from './readState';

describe('folderBearsReadState', () => {
  it('is true for received-mail folders', () => {
    expect(folderBearsReadState('inbox')).toBe(true);
    expect(folderBearsReadState('archive')).toBe(true);
    expect(folderBearsReadState('skywarn-net')).toBe(true); // a user-folder slug
  });
  it("is false for the operator's own / non-received folders", () => {
    for (const f of ['sent', 'outbox', 'drafts', 'deleted']) {
      expect(folderBearsReadState(f)).toBe(false);
    }
  });
});
