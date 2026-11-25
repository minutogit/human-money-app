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

  it('supports removeItem and clear without leaking keys across operations', () => {
    try {
      localStorage.setItem('alpha-key', 'alpha');
      localStorage.setItem('beta-key', 'beta');
      expect(localStorage.length).toBe(2);

      localStorage.removeItem('alpha-key');
      expect(localStorage.getItem('alpha-key')).toBeNull();
      expect(localStorage.getItem('beta-key')).toBe('beta');
      expect(localStorage.length).toBe(1);

      localStorage.clear();
      expect(localStorage.length).toBe(0);
      expect(localStorage.getItem('beta-key')).toBeNull();
    } finally {
      localStorage.clear();
    }
  });
});
