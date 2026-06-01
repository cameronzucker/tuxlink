import { describe, it, expect } from 'vitest';
import './ics213';
import './bulletin';
import './position';
import './ics309';
import './damage_assessment';
import { lookupForm, allForms, composableForms } from './forms';

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

describe('composableForms', () => {
  it('returns only forms with a Form component (picker scope)', () => {
    const composable = composableForms();
    const composableIds = composable.map((f) => f.id);
    expect(composableIds).toContain('ICS213_Initial');
    expect(composableIds).toContain('Bulletin_Initial');
    // NOTE: the Position/Form-309/Damage_Assessment forms still register with
    // their Form component at the time this test was written (Task 1). After
    // Tasks 3-5 strip their Form registration, this test will be extended to
    // assert `.not.toContain` for those ids. For now, only the positive
    // assertions are made.
    // expect(composableIds).not.toContain('Position_Report');
    // expect(composableIds).not.toContain('Form-309_Initial');
    // expect(composableIds).not.toContain('Damage_Assessment_Initial');
  });

  it('still allows lookupForm to find view-only entries', () => {
    expect(lookupForm('Position_Report')).toBeDefined();
    expect(lookupForm('Form-309_Initial')).toBeDefined();
    expect(lookupForm('Damage_Assessment_Initial')).toBeDefined();
  });
});
