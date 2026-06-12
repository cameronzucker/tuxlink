import { describe, it, expect } from 'vitest';
import { getTopicBySlug } from './topics';

// G11 (tuxlink-48uc): a stuck onboarding member must be able to find the
// import flow in the help corpus.
describe('forms import help', () => {
  it('the HTML forms topic documents importing group forms', () => {
    const topic = getTopicBySlug('20-html-forms');
    expect(topic).toBeDefined();
    expect(topic!.body).toContain('Import group forms');
    expect(topic!.body).toContain('Open forms folder');
    expect(topic!.body.toLowerCase()).toContain('remove');
  });
});
