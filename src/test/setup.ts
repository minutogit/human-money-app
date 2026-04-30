import { afterEach, vi } from 'vitest';
import { cleanup, configure } from '@testing-library/react';
import '@testing-library/jest-dom';

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
