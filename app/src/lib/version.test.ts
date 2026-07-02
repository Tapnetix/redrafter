import { describe, expect, it } from 'vitest';
import { APP } from './version';

describe('APP', () => {
  it('identifies the application as redrafter', () => {
    expect(APP).toBe('redrafter');
  });
});
