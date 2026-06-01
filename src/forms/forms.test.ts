import { describe, it, expect } from 'vitest';
import './ics213';
import { lookupForm, allForms } from './forms';

describe('forms registry', () => {
  it('finds Ics213 after import', () => {
    const entry = lookupForm('ICS213_Initial');
    expect(entry).toBeDefined();
    expect(entry?.name).toBe('ICS-213 General Message');
  });

  it('lists all registered forms', () => {
    const list = allForms();
    expect(list.find((f) => f.id === 'ICS213_Initial')).toBeDefined();
  });
});
