import '@testing-library/jest-dom/vitest';
import { afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';

// globals:false means testing-library's automatic afterEach(cleanup) isn't
// registered; do it explicitly so the jsdom DOM is reset between tests.
afterEach(() => cleanup());
