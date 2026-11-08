/* eslint-disable */
import { afterEach, vi } from 'vitest';
import { cleanup, configure } from '@testing-library/react';
import '@testing-library/jest-dom';

const enTranslations = vi.hoisted(() => {
  const fs = require('fs');
  const path = require('path');
  const enPath = path.resolve(process.cwd(), 'src/locales/en.json');
  try {
    return JSON.parse(fs.readFileSync(enPath, 'utf-8'));
  } catch (err) {
    console.error('Error loading enTranslations in setup.ts:', err);
    return {};
  }
});

// Node 26 exposes a localStorage getter even without a backing file. Vitest 2
// preserves that existing global instead of installing JSDOM's browser storage.
// Use the real storage of this test environment, never Node's process storage.
const browserWindow = (globalThis as typeof globalThis & {
    jsdom: { window: Window };
}).jsdom.window;
Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    writable: true,
    value: browserWindow.localStorage,
});

// Limit the size of the DOM dump in error messages to keep output readable
configure({
    // This prevents the huge HTML dump on failure
    getElementError: (message) => {
        const error = new Error(message || 'Element not found');
        error.name = 'TestingLibraryElementError';
        return error;
    }
});

// Cleanup after each test
afterEach(() => {
  cleanup();
});

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-log', () => ({
  info: vi.fn(),
  error: vi.fn(),
  warn: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: vi.fn(),
  writeFile: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: vi.fn().mockResolvedValue(undefined),
  readText: vi.fn().mockResolvedValue(''),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  revealItemInDir: vi.fn().mockResolvedValue(undefined),
  openPath: vi.fn().mockResolvedValue(undefined),
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

// Mock settingsUtils
vi.mock('../utils/settingsUtils', () => ({
  updateLastUsedDirectory: vi.fn(),
}));

// Mock react-i18next to return English translations
const stableT = (key: string, defaultOrOptions?: any, options?: any) => {
  const actualOptions = typeof defaultOrOptions === 'object' ? defaultOrOptions : options;
  const defaultVal = typeof defaultOrOptions === 'string' ? defaultOrOptions : key;
  let val = (enTranslations as Record<string, string>)[key] || defaultVal;
  if (actualOptions && typeof actualOptions === 'object') {
    for (const [k, v] of Object.entries(actualOptions)) {
      val = val.replace(new RegExp(`\\{\\{\\s*${k}\\s*\\}\\}`, 'g'), String(v));
    }
  }
  return val;
};

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: stableT,
    i18n: { language: 'en', changeLanguage: vi.fn() },
    ready: true,
  }),
}));
