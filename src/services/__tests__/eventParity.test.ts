import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const CURRENT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(CURRENT_DIR, "../../..");
const RUST_SRC_ROOT = path.join(APP_ROOT, "src-tauri/src");
const SRC_ROOT = path.join(APP_ROOT, "src");

function walkFiles(dir: string, exts: string[], excludeDirs: string[] = []): string[] {
    const out: string[] = [];
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
            if (excludeDirs.includes(entry.name)) continue;
            out.push(...walkFiles(full, exts, excludeDirs));
        } else if (
            exts.some((ext) => entry.name.endsWith(ext)) &&
            !/\.test\.(ts|tsx)$/.test(entry.name)
        ) {
            out.push(full);
        }
    }
    return out;
}

function findMatchingBraceEnd(src: string, openIdx: number): number {
    let depth = 0;
    let inString = false;
    let inLineComment = false;
    let inBlockComment = false;
    for (let i = openIdx; i < src.length; i++) {
        const ch = src[i];
        const next = src[i + 1];
        if (inLineComment) {
            if (ch === "\n") inLineComment = false;
            continue;
        }
        if (inBlockComment) {
            if (ch === "*" && next === "/") {
                inBlockComment = false;
                i++;
            }
            continue;
        }
        if (inString) {
            if (ch === "\\") {
                i++;
                continue;
            }
            if (ch === '"') inString = false;
            continue;
        }
        if (ch === "/" && next === "/") {
            inLineComment = true;
            i++;
            continue;
        }
        if (ch === "/" && next === "*") {
            inBlockComment = true;
            i++;
            continue;
        }
        if (ch === '"') {
            inString = true;
            continue;
        }
        if (ch === "'" && next === "\\" && src[i + 3] === "'") {
            i += 3;
            continue;
        }
        if (ch === "'" && next !== "'" && next !== "\\" && src[i + 2] === "'") {
            i += 2;
            continue;
        }
        if (ch === "{") depth++;
        else if (ch === "}") {
            depth--;
            if (depth === 0) return i;
        }
    }
    return src.length;
}

function stripRustTestsAndComments(src: string): string {
    let cleaned = src;
    const cfgModRe = /#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+[a-zA-Z0-9_]+\s*\{/g;
    for (const m of [...src.matchAll(cfgModRe)].reverse()) {
        const start = m.index ?? 0;
        const openIdx = src.indexOf("{", start);
        if (openIdx === -1) continue;
        const endIdx = findMatchingBraceEnd(src, openIdx);
        cleaned = cleaned.slice(0, start) + cleaned.slice(endIdx + 1);
    }
    let result = "";
    let inString = false;
    let inLineComment = false;
    let inBlockComment = false;
    for (let i = 0; i < cleaned.length; i++) {
        const ch = cleaned[i];
        const next = cleaned[i + 1];
        if (inLineComment) {
            if (ch === "\n") {
                inLineComment = false;
                result += ch;
            }
            continue;
        }
        if (inBlockComment) {
            if (ch === "*" && next === "/") {
                inBlockComment = false;
                i++;
            }
            continue;
        }
        if (inString) {
            result += ch;
            if (ch === "\\" && next !== undefined) {
                result += next;
                i++;
                continue;
            }
            if (ch === '"') inString = false;
            continue;
        }
        if (ch === "/" && next === "/") {
            inLineComment = true;
            i++;
            continue;
        }
        if (ch === "/" && next === "*") {
            inBlockComment = true;
            i++;
            continue;
        }
        if (ch === '"') inString = true;
        result += ch;
    }
    return result;
}

describe("Tauri backend/frontend event parity guard", () => {
    it("maintains exact 1:1 parity between Rust emit('humoco://...') and TS listen('humoco://...') events", () => {
        const rustEvents = new Set<string>();
        const emitRe = /\.emit(?:_to|_filter)?\s*\([^;)]*?"(humoco:\/\/[^"]+)"/g;
        for (const file of walkFiles(RUST_SRC_ROOT, [".rs"])) {
            const src = stripRustTestsAndComments(fs.readFileSync(file, "utf-8"));
            for (const m of src.matchAll(emitRe)) {
                rustEvents.add(m[1]);
            }
        }

        const tsEvents = new Set<string>();
        const listenRe = /\b(?:listen|register)\s*(?:<[^>]*>)?\s*\(\s*(['"])(humoco:\/\/[^'"]+)\1/g;
        for (const file of walkFiles(SRC_ROOT, [".ts", ".tsx"], ["__tests__", "node_modules"])) {
            const src = fs.readFileSync(file, "utf-8");
            for (const m of src.matchAll(listenRe)) {
                tsEvents.add(m[2]);
            }
        }

        const rustSorted = [...rustEvents].sort();
        const tsSorted = [...tsEvents].sort();

        expect(rustSorted.length).toBeGreaterThan(0);
        expect(tsSorted).toEqual(rustSorted);
    });

    it("includes all 5 core background coordinator events in both Rust and TS", () => {
        const expectedCoreEvents = [
            "humoco://backup-updated",
            "humoco://sync-status-updated",
            "humoco://post-synced",
            "humoco://market-synced",
            "humoco://conflicts-updated",
        ];

        const rustEvents = new Set<string>();
        const emitRe = /\.emit(?:_to|_filter)?\s*\([^;)]*?"(humoco:\/\/[^"]+)"/g;
        for (const file of walkFiles(RUST_SRC_ROOT, [".rs"])) {
            const src = stripRustTestsAndComments(fs.readFileSync(file, "utf-8"));
            for (const m of src.matchAll(emitRe)) {
                rustEvents.add(m[1]);
            }
        }

        const tsEvents = new Set<string>();
        const listenRe = /\b(?:listen|register)\s*(?:<[^>]*>)?\s*\(\s*(['"])(humoco:\/\/[^'"]+)\1/g;
        for (const file of walkFiles(SRC_ROOT, [".ts", ".tsx"], ["__tests__", "node_modules"])) {
            const src = fs.readFileSync(file, "utf-8");
            for (const m of src.matchAll(listenRe)) {
                tsEvents.add(m[2]);
            }
        }

        for (const ev of expectedCoreEvents) {
            expect(rustEvents.has(ev)).toBe(true);
            expect(tsEvents.has(ev)).toBe(true);
        }
    });
});
