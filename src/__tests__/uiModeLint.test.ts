import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const SRC_ROOT = path.resolve(__dirname, '..');

/**
 * Only these paths (excluding test files) may import or call `useUiMode`.
 * Views must use <Advanced>, <AdvancedDisclosure>, <TechId>, <PresetOrCustom>,
 * PageLayout.advancedActions, NavItem.mode, or HelpTopic.audience instead.
 */
const ALLOWED_RELATIVE_PREFIXES = [
    'context/UiModeContext.tsx',
    'components/ui/',
    'components/shell/Sidebar.tsx',
    'components/shell/Header.tsx',
    'components/support/HelpCenterView.tsx',
];

function collectSourceFiles(dir: string): string[] {
    const entries = fs.readdirSync(dir, { withFileTypes: true });
    const files: string[] = [];

    for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory()) {
            if (entry.name === '__tests__' || entry.name === 'node_modules') {
                continue;
            }
            files.push(...collectSourceFiles(fullPath));
        } else if (entry.isFile() && /\.(ts|tsx)$/.test(entry.name)) {
            if (/\.test\.(ts|tsx)$/.test(entry.name)) {
                continue;
            }
            files.push(fullPath);
        }
    }

    return files;
}

describe('UI Mode Architecture Guardrail (APP-UX-09)', () => {
    it('restricts direct useUiMode(), UiModeContext, and .uiMode reads to allowed primitives and shell components', () => {
        const files = collectSourceFiles(SRC_ROOT);
        const useUiModeViolations: string[] = [];
        const contextImportViolations: string[] = [];
        const propertyAccessViolations: string[] = [];

        for (const file of files) {
            const relPath = path.relative(SRC_ROOT, file).split(path.sep).join('/');
            const isAllowed = ALLOWED_RELATIVE_PREFIXES.some((prefix) =>
                prefix.endsWith('/') ? relPath.startsWith(prefix) : relPath === prefix
            );
            if (isAllowed) continue;

            const content = fs.readFileSync(file, 'utf8');
            if (/\buseUiMode\b/.test(content)) {
                useUiModeViolations.push(relPath);
            }
            if (/\bUiModeContext\b/.test(content) && relPath !== 'App.tsx') {
                contextImportViolations.push(relPath);
            }
            if (
                /\.uiMode\b/.test(content) &&
                relPath !== 'utils/tourBridge.ts' &&
                !relPath.startsWith('types/')
            ) {
                propertyAccessViolations.push(relPath);
            }
        }

        expect(files.length).toBeGreaterThan(50);
        for (const prefix of ALLOWED_RELATIVE_PREFIXES) {
            expect(fs.existsSync(path.join(SRC_ROOT, prefix))).toBe(true);
        }

        expect(useUiModeViolations).toEqual([]);
        expect(contextImportViolations).toEqual([]);
        expect(propertyAccessViolations).toEqual([]);
    });

    it('detects direct UiMode escape hatches while allowing Primitive components', () => {
        const unsafeHook = 'const { mode } = useUiMode();';
        const unsafeContext = 'import { UiModeContext } from "../context/UiModeContext";';
        const unsafeProp = 'if (config.uiPreferences.uiMode === "expert") {}';
        const safePrimitive = '<Advanced><TechId value="did:key:z1" uiModeLabel="x" /></Advanced>';

        expect(/\buseUiMode\b/.test(unsafeHook)).toBe(true);
        expect(/\bUiModeContext\b/.test(unsafeContext)).toBe(true);
        expect(/\.uiMode\b/.test(unsafeProp)).toBe(true);
        expect(/\buseUiMode\b/.test(safePrimitive)).toBe(false);
        expect(/\bUiModeContext\b/.test(safePrimitive)).toBe(false);
        expect(/\.uiMode\b/.test(safePrimitive)).toBe(false);
    });
});
