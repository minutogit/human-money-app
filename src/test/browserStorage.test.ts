import { describe, expect, it } from 'vitest';

describe('JSDOM browser storage fixture', () => {
  it('provides real origin storage instead of the Node process getter', () => {
    expect(localStorage).toBeInstanceOf(Storage);
    const key = 'humoco-browser-storage-fixture';
    expect(localStorage.getItem(key)).toBeNull();
    try {
      localStorage.setItem(key, 'browser-value');
      expect(localStorage.getItem(key)).toBe('browser-value');
      expect(localStorage.key(0)).toBe(key);
      expect(localStorage.length).toBe(1);
    } finally {
      localStorage.removeItem(key);
    }
    expect(localStorage.getItem(key)).toBeNull();
  });
});
