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
    'components/Sidebar.tsx',
    'components/Header.tsx',
    'components/HelpCenterView.tsx',
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
    it('restricts direct useUiMode() usage to UiModeContext, UI primitives, Header, Sidebar, and HelpCenterView', () => {
        const files = collectSourceFiles(SRC_ROOT);
        const violations: string[] = [];

        for (const file of files) {
            const relPath = path.relative(SRC_ROOT, file).split(path.sep).join('/');
            const isAllowed = ALLOWED_RELATIVE_PREFIXES.some((prefix) =>
                prefix.endsWith('/') ? relPath.startsWith(prefix) : relPath === prefix
            );
            if (isAllowed) continue;

            const content = fs.readFileSync(file, 'utf8');
            if (/\buseUiMode\b/.test(content)) {
                violations.push(relPath);
            }
        }

        expect(violations).toEqual([]);
    });
});
