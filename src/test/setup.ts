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
  writeText: vi.fn(),
}));

// Mock settingsUtils
vi.mock('../utils/settingsUtils', () => ({
  updateLastUsedDirectory: vi.fn(),
}));

// Mock react-i18next to return English translations
const stableT = (key: string, options?: any) => {
  let val = (enTranslations as Record<string, string>)[key] || key;
  if (options && typeof options === 'object') {
    for (const [k, v] of Object.entries(options)) {
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
