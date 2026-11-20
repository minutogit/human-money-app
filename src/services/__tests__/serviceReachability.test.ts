import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { COMMAND_ACCESS_POLICY } from "../commandPolicy";

const APP_ROOT = path.resolve(__dirname, "../../..");
const LIB_RS_PATH = path.join(APP_ROOT, "src-tauri/src/lib.rs");
const SRC_ROOT = path.join(APP_ROOT, "src");

function extractRegisteredCommands(libRs: string): string[] {
    const match = libRs.match(/tauri::generate_handler!\[([\s\S]*?)\]/);
    if (!match) {
        throw new Error("Could not find tauri::generate_handler! in src-tauri/src/lib.rs");
    }
    return match[1]
        .split("\n")
        .map((line) => line.replace(/\/\/.*$/, "").trim().replace(/,$/, ""))
        .filter(Boolean);
}

function collectProductionTsFiles(dir: string, out: string[] = []): string[] {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
            if (entry.name === "__tests__" || entry.name === "node_modules") continue;
            collectProductionTsFiles(full, out);
        } else if (/\.(ts|tsx)$/.test(entry.name) && !/\.test\.(ts|tsx)$/.test(entry.name)) {
            out.push(full);
        }
    }
    return out;
}

describe("Backend command & service reachability guard", () => {
    const libRs = fs.readFileSync(LIB_RS_PATH, "utf-8");
    const registeredCommands = extractRegisteredCommands(libRs).sort();
    const prodFiles = collectProductionTsFiles(SRC_ROOT);
    const prodContents = new Map(prodFiles.map((f) => [f, fs.readFileSync(f, "utf-8")]));

    it("maintains exact 1:1 parity between lib.rs commands, COMMAND_ACCESS_POLICY, and production src/ invoke() calls", () => {
        const invokedCommands = new Set<string>();
        const dynamicInvokes: string[] = [];

        const literalInvokeRe = /\binvoke(?:<[^>]*>)?\s*\(\s*(['"])([a-z0-9_]+)\1/g;
        const genericInvokeRe = /\binvoke(?:<[^>]*>)?\s*\(\s*([^\s'"),][^,)\n]*)/g;

        for (const [filePath, content] of prodContents) {
            for (const m of content.matchAll(literalInvokeRe)) {
                invokedCommands.add(m[2]);
            }
            for (const m of content.matchAll(genericInvokeRe)) {
                const rel = path.relative(APP_ROOT, filePath);
                dynamicInvokes.push(`${rel}: ${m[0].trim()}`);
            }
        }

        const invokedSorted = [...invokedCommands].sort();
        const policySorted = Object.keys(COMMAND_ACCESS_POLICY).sort();

        expect(dynamicInvokes).toEqual([]);
        expect(invokedSorted).toEqual(registeredCommands);
        expect(policySorted).toEqual(registeredCommands);
    });

    it("ensures removed legacy/bypass commands remain completely absent from backend and frontend", () => {
        const deletedCommands = [
            "checkout_received_bundle",
            "save_transaction_record",
            "take_startup_launch_items",
        ];

        for (const cmd of deletedCommands) {
            expect(registeredCommands).not.toContain(cmd);
            expect(Object.keys(COMMAND_ACCESS_POLICY)).not.toContain(cmd);
        }

        const bannedInProd = [...deletedCommands, "checkoutReceivedBundle"];
        for (const [filePath, content] of prodContents) {
            const rel = path.relative(APP_ROOT, filePath);
            for (const sym of bannedInProd) {
                expect(content, `Removed symbol '${sym}' still present in ${rel}`).not.toContain(sym);
            }
        }
    });

    it("ensures key refactored service entrypoints are wired into production callers", () => {
        const expectedEntrypoints: Array<{ symbol: string; definedIn: string }> = [
            { symbol: "receiveBundle", definedIn: "src/services/transferService.ts" },
            { symbol: "createBundle", definedIn: "src/services/transferService.ts" },
            { symbol: "depositTransferToPost", definedIn: "src/services/postService.ts" },
            { symbol: "updateNetworkStatus", definedIn: "src/sync/syncBridge.ts" },
            { symbol: "afterWalletMutation", definedIn: "src/sync/syncBridge.ts" },
        ];

        for (const { symbol, definedIn } of expectedEntrypoints) {
            const re = new RegExp(`\\b${symbol}\\b`);
            const callers = [...prodContents.entries()]
                .filter(([filePath, content]) => {
                    const rel = path.relative(APP_ROOT, filePath);
                    return rel !== definedIn && re.test(content);
                })
                .map(([filePath]) => path.relative(APP_ROOT, filePath));

            expect(
                callers.length,
                `Expected at least one production caller outside ${definedIn} for '${symbol}'`,
            ).toBeGreaterThan(0);
        }
    });
});
