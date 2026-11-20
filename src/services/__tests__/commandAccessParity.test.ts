// src/services/__tests__/commandAccessParity.test.ts
import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import { describe, expect, it } from "vitest";
import {
    COMMAND_ACCESS_POLICY,
    EXPLICIT_PASSWORD_REAUTH_COMMANDS,
    protectCommandOptions,
    type CommandAccessTier,
} from "../commandPolicy";

const APP_ROOT = path.resolve(__dirname, "../../..");
const LIB_RS_PATH = path.join(APP_ROOT, "src-tauri/src/lib.rs");
const RUST_SRC_ROOT = path.join(APP_ROOT, "src-tauri/src");
const SRC_ROOT = path.join(APP_ROOT, "src");

const TIER_RANK: Record<CommandAccessTier, number> = {
    public: 0,
    unlocked: 1,
    dailyPay: 2,
    full: 3,
};

function walkFiles(dir: string, exts: string[], excludeDirs: string[] = []): string[] {
    const out: string[] = [];
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        if (entry.isDirectory()) {
            if (excludeDirs.includes(entry.name)) continue;
            out.push(...walkFiles(path.join(dir, entry.name), exts, excludeDirs));
        } else if (exts.some((ext) => entry.name.endsWith(ext))) {
            out.push(path.join(dir, entry.name));
        }
    }
    return out;
}

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
            if (ch === '"') {
                inString = false;
            }
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
        // Skip Rust char literals ('{', '\n', etc.) without swallowing lifetime markers ('a, 'static).
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

function stripRustCfgTestModules(src: string): string {
    let cleaned = src;
    const cfgModRe = /#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+[a-zA-Z0-9_]+\s*\{/g;
    for (const m of [...src.matchAll(cfgModRe)].reverse()) {
        const start = m.index ?? 0;
        const braceOpen = src.indexOf("{", start);
        if (braceOpen === -1) continue;
        const braceEnd = findMatchingBraceEnd(src, braceOpen);
        cleaned = cleaned.slice(0, start) + cleaned.slice(braceEnd + 1);
    }
    // Also strip single-function #[cfg(test)] items if any remain
    return cleaned.replace(/\/\/.*$/gm, "");
}

function extractRustFunctions(rustFiles: string[]): {
    commands: Map<string, string>;
    allFns: Map<string, string[]>;
} {
    const commands = new Map<string, string>();
    const allFns = new Map<string, string[]>();
    const fnRegex = /(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*(?:<[^>]*>)?\s*\(/g;

    for (const file of rustFiles) {
        const src = stripRustCfgTestModules(fs.readFileSync(file, "utf-8"));
        for (const m of src.matchAll(fnRegex)) {
            const fnName = m[1];
            const fnStart = m.index ?? 0;
            const parenOpen = src.indexOf("(", fnStart);
            if (parenOpen === -1) continue;
            let pDepth = 0;
            let braceSearchStart = parenOpen;
            for (let i = parenOpen; i < src.length; i++) {
                if (src[i] === "(") pDepth++;
                else if (src[i] === ")") {
                    pDepth--;
                    if (pDepth === 0) {
                        braceSearchStart = i + 1;
                        break;
                    }
                }
            }
            const semiIdx = src.indexOf(";", braceSearchStart);
            const openBraceIdx = src.indexOf("{", braceSearchStart);
            if (openBraceIdx === -1 || (semiIdx !== -1 && semiIdx < openBraceIdx)) continue;
            const bodyEnd = findMatchingBraceEnd(src, openBraceIdx);
            const body = src.slice(openBraceIdx, bodyEnd + 1);
            const prevBlock = src.slice(Math.max(0, fnStart - 120), fnStart);
            const existing = allFns.get(fnName) ?? [];
            existing.push(body);
            allFns.set(fnName, existing);
            if (/#\[tauri::command\]/.test(prevBlock)) {
                commands.set(fnName, body);
            }
        }
    }
    return { commands, allFns };
}

function detectDirectRustTier(body: string): CommandAccessTier | null {
    let maxTier: CommandAccessTier | null = null;
    for (const m of body.matchAll(/RequiredAccess::(Unlocked|DailyPay|Full)\b/g)) {
        const mapped: CommandAccessTier =
            m[1] === "Full" ? "full" : m[1] === "DailyPay" ? "dailyPay" : "unlocked";
        if (!maxTier || TIER_RANK[mapped] > TIER_RANK[maxTier]) {
            maxTier = mapped;
        }
    }
    return maxTier;
}

function extractRustCommandTierMap(): Map<string, CommandAccessTier> {
    const rustFiles = walkFiles(RUST_SRC_ROOT, [".rs"]);
    const { commands, allFns } = extractRustFunctions(rustFiles);
    const result = new Map<string, CommandAccessTier>();

    const checkBodyTier = (body: string): CommandAccessTier | null => {
        let best = detectDirectRustTier(body);
        // Also recognize explicit admin-key helper and Core full delete verification
        if (/\bload_admin_with_signing_key\s*\(/.test(body) || /service\s*\.\s*delete_profile\s*\(/.test(body)) {
            if (!best || TIER_RANK.full > TIER_RANK[best]) best = "full";
        }
        return best;
    };

    for (const [cmdName, cmdBody] of commands.entries()) {
        let tier = checkBodyTier(cmdBody);
        const visited = new Set<string>();
        const queue: string[] = [];
        if (!/\binitialize_profile_session\s*\(/.test(cmdBody)) {
            queue.push(`${cmdName}_impl`);
            for (const m of cmdBody.matchAll(/\b([a-z0-9_]+_impl|replace_local_profile_with_restored)\s*\(/g)) {
                queue.push(m[1]);
            }
        }
        while (queue.length > 0) {
            const target = queue.shift()!;
            if (visited.has(target)) continue;
            visited.add(target);
            const bodies = allFns.get(target) ?? [];
            for (const b of bodies) {
                const subTier = checkBodyTier(b);
                if (subTier && (!tier || TIER_RANK[subTier] > TIER_RANK[tier])) {
                    tier = subTier;
                }
                // Public onboarding/recovery bootstrap functions that establish a new session
                // call post-onboarding helpers directly with the just-verified credentials.
                if (!/\binitialize_profile_session\s*\(/.test(b)) {
                    for (const m2 of b.matchAll(/\b([a-z0-9_]+_impl|load_admin_with_signing_key)\s*\(/g)) {
                        if (!visited.has(m2[1])) queue.push(m2[1]);
                    }
                }
            }
        }
        if (tier) {
            result.set(cmdName, tier);
        }
    }
    return result;
}

function buildServiceMethodToCommandMap(): Map<string, string> {
    const serviceFiles = walkFiles(path.join(SRC_ROOT, "services"), [".ts"], ["__tests__"]);
    const methodMap = new Map<string, string>();

    for (const file of serviceFiles) {
        const src = fs.readFileSync(file, "utf-8");
        const sf = ts.createSourceFile(file, src, ts.ScriptTarget.Latest, true);

        const visit = (node: ts.Node) => {
            if (
                ts.isVariableDeclaration(node) &&
                ts.isIdentifier(node.name) &&
                node.name.text.endsWith("Service") &&
                node.initializer &&
                ts.isObjectLiteralExpression(node.initializer)
            ) {
                const svcName = node.name.text;
                for (const prop of node.initializer.properties) {
                    let propName: string | null = null;
                    let initNode: ts.Node | undefined;
                    if (ts.isPropertyAssignment(prop) && ts.isIdentifier(prop.name)) {
                        propName = prop.name.text;
                        initNode = prop.initializer;
                    } else if (ts.isMethodDeclaration(prop) && ts.isIdentifier(prop.name)) {
                        propName = prop.name.text;
                        initNode = prop.body;
                    }
                    if (!propName || !initNode) continue;

                    const invoked: string[] = [];
                    const walkBody = (n: ts.Node) => {
                        if (
                            ts.isCallExpression(n) &&
                            ts.isIdentifier(n.expression) &&
                            n.expression.text === "invoke" &&
                            n.arguments.length >= 1 &&
                            ts.isStringLiteral(n.arguments[0])
                        ) {
                            invoked.push(n.arguments[0].text);
                        }
                        ts.forEachChild(n, walkBody);
                    };
                    walkBody(initNode);
                    if (invoked.length === 1) {
                        methodMap.set(`${svcName}.${propName}`, invoked[0]);
                    }
                }
            }
            ts.forEachChild(node, visit);
        };
        visit(sf);
    }
    return methodMap;
}

function hasFullGateOptions(optArg: ts.Expression | undefined): boolean {
    if (!optArg) return false;
    if (
        ts.isCallExpression(optArg) &&
        ts.isIdentifier(optArg.expression) &&
        optArg.expression.text === "protectCommandOptions" &&
        optArg.arguments.length >= 1 &&
        ts.isStringLiteral(optArg.arguments[0])
    ) {
        const cmd = optArg.arguments[0].text as keyof typeof COMMAND_ACCESS_POLICY;
        return COMMAND_ACCESS_POLICY[cmd] === "full";
    }
    if (ts.isObjectLiteralExpression(optArg)) {
        return optArg.properties.some(
            (p) =>
                ts.isPropertyAssignment(p) &&
                ts.isIdentifier(p.name) &&
                p.name.text === "requireFullAccess" &&
                p.initializer.kind === ts.SyntaxKind.TrueKeyword,
        );
    }
    return false;
}

function getCalleeServiceKey(call: ts.CallExpression): string | null {
    const expr = call.expression;
    if (ts.isPropertyAccessExpression(expr) && ts.isIdentifier(expr.expression) && ts.isIdentifier(expr.name)) {
        return `${expr.expression.text}.${expr.name.text}`;
    }
    return null;
}

function isProtectActionCall(call: ts.CallExpression): {
    isGate: boolean;
    cbArg?: ts.Expression;
    optsArg?: ts.Expression;
} {
    const expr = call.expression;
    let name: string | null = null;
    if (ts.isIdentifier(expr)) name = expr.text;
    else if (ts.isPropertyAccessExpression(expr) && ts.isIdentifier(expr.name)) {
        name = expr.name.text;
    }
    if (name === "protectAction" && call.arguments.length >= 1) {
        return { isGate: true, cbArg: call.arguments[0], optsArg: call.arguments[1] };
    }
    if (name === "runProtected" && call.arguments.length >= 1) {
        // runProtected(cb, opts) in ReachPresetSelector OR runProtected(busySetter, cb, opts) in useSyncServiceAdmin
        const first = call.arguments[0];
        if (ts.isArrowFunction(first) || ts.isFunctionExpression(first)) {
            return { isGate: true, cbArg: first, optsArg: call.arguments[1] };
        }
        return { isGate: true, cbArg: call.arguments[1], optsArg: call.arguments[2] };
    }
    return { isGate: false };
}

function hasExplicitPasswordArgument(call: ts.CallExpression, sf: ts.SourceFile): boolean {
    if (call.arguments.length === 0) return false;
    // Object arg with explicit `password` / `masterPassword` field, OR positional `password`/`masterPassword` identifier
    const first = call.arguments[0];
    if (ts.isIdentifier(first) && /password/i.test(first.text)) return true;
    if (ts.isObjectLiteralExpression(first)) {
        return first.properties.some((p) => {
            if (ts.isShorthandPropertyAssignment(p)) {
                return /password/i.test(p.name.text);
            }
            if (ts.isPropertyAssignment(p) && ts.isIdentifier(p.name) && /password/i.test(p.name.text)) {
                const txt = p.initializer.getText(sf);
                return txt !== "undefined" && txt !== "null" && !txt.includes("|| undefined");
            }
            return false;
        });
    }
    return false;
}

describe("Tauri command access policy parity (F3)", () => {
    it("covers every registered Tauri command in src-tauri/src/lib.rs with zero drift", () => {
        const libRs = fs.readFileSync(LIB_RS_PATH, "utf-8");
        const registered = extractRegisteredCommands(libRs).sort();
        const policyKeys = Object.keys(COMMAND_ACCESS_POLICY).sort();

        expect(policyKeys).toEqual(registered);
    });

    it("matches backend RequiredAccess tiers for every command and enforces export_wallet_backup_file as full", () => {
        expect(COMMAND_ACCESS_POLICY.export_wallet_backup_file).toBe("full");
        expect(COMMAND_ACCESS_POLICY.revoke_own_sync_server).toBe("full");
        expect(COMMAND_ACCESS_POLICY.confirm_pending_post_payment).toBe("dailyPay");

        const rustTiers = extractRustCommandTierMap();
        const mismatches: string[] = [];
        for (const [cmd, rustTier] of rustTiers.entries()) {
            const policyTier = COMMAND_ACCESS_POLICY[cmd as keyof typeof COMMAND_ACCESS_POLICY];
            if (policyTier !== rustTier) {
                mismatches.push(`${cmd}: backend=${rustTier}, commandPolicy=${policyTier}`);
            }
        }
        expect(mismatches).toEqual([]);
    });

    it("maps protectCommandOptions to requireFullAccess only for Full-tier commands", () => {
        expect(protectCommandOptions("create_new_voucher")).toEqual({ requireFullAccess: true });
        expect(protectCommandOptions("export_wallet_backup_file")).toEqual({ requireFullAccess: true });
        expect(protectCommandOptions("save_public_sync")).toEqual({ requireFullAccess: true });
        expect(protectCommandOptions("register_guarantor_anchor_role")).toEqual({
            requireFullAccess: true,
        });
        expect(protectCommandOptions("sync_wallet_backup_folder")).toEqual({
            requireFullAccess: true,
        });
        expect(protectCommandOptions("resolve_card_recipient")).toEqual({});
        expect(protectCommandOptions("create_transfer_bundle")).toEqual({});
        expect(protectCommandOptions("confirm_pending_post_payment")).toEqual({});
        expect(protectCommandOptions("dismiss_wallet_sync_alert")).toEqual({});
        expect(protectCommandOptions("get_app_config")).toEqual({});
        expect(protectCommandOptions("list_profiles")).toEqual({});
    });

    it("enforces call-site Full-tier gates, single-call DailyPay protectAction blocks (H1), and explicit deleteProfile re-auth (H3)", () => {
        const serviceToCmd = buildServiceMethodToCommandMap();
        const uiFiles = walkFiles(SRC_ROOT, [".ts", ".tsx"], ["__tests__", "services", "locales"]);

        const unGatedFullCalls: string[] = [];
        const multiDailyPayBlocks: string[] = [];
        const badDeleteCalls: string[] = [];

        for (const file of uiFiles) {
            const rel = path.relative(SRC_ROOT, file);
            const src = fs.readFileSync(file, "utf-8");
            const sf = ts.createSourceFile(file, src, ts.ScriptTarget.Latest, true);
            const fileHasFullPrecheck =
                src.includes("ensureFullAccess") || src.includes("requireFullAccess");

            // Track local helper functions called inside Full-gated protectAction callbacks vs outside.
            const localFnDefs = new Map<string, ts.Node[]>();
            const fullGatedHelpers = new Set<string>();
            const ungatedHelpers = new Set<string>();
            const skippedFallbackBranches = new Set<ts.Node>();

            const registerLocalFn = (name: string, fnNode: ts.Node) => {
                const list = localFnDefs.get(name) ?? [];
                list.push(fnNode);
                localFnDefs.set(name, list);
            };

            const collectFns = (n: ts.Node) => {
                if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && n.initializer) {
                    if (
                        ts.isArrowFunction(n.initializer) ||
                        ts.isFunctionExpression(n.initializer)
                    ) {
                        registerLocalFn(n.name.text, n.initializer);
                    }
                } else if (ts.isFunctionDeclaration(n) && n.name && n.body) {
                    registerLocalFn(n.name.text, n);
                }
                ts.forEachChild(n, collectFns);
            };
            collectFns(sf);

            const containsFullGate = (subtree: ts.Node): boolean => {
                let found = false;
                const walk = (n: ts.Node) => {
                    if (found) return;
                    if (ts.isCallExpression(n)) {
                        const pa = isProtectActionCall(n);
                        if (pa.isGate && hasFullGateOptions(pa.optsArg)) {
                            found = true;
                            return;
                        }
                    }
                    ts.forEachChild(n, walk);
                };
                walk(subtree);
                return found;
            };

            const collectHelperCalls = (n: ts.Node, inFullGate: boolean) => {
                if (skippedFallbackBranches.has(n)) return;

                // Recognize `session?.protectAction ? session.protectAction(..., fullOpts) : helper(...)`
                // and `if (session?.protectAction) { ...fullGate... } else { helper(...); }` optional-session wrappers.
                if (
                    ts.isConditionalExpression(n) &&
                    /protectAction/.test(n.condition.getText(sf)) &&
                    containsFullGate(n.whenTrue)
                ) {
                    skippedFallbackBranches.add(n.whenFalse);
                } else if (
                    ts.isIfStatement(n) &&
                    n.elseStatement &&
                    /protectAction/.test(n.expression.getText(sf)) &&
                    containsFullGate(n.thenStatement)
                ) {
                    skippedFallbackBranches.add(n.elseStatement);
                }

                if (ts.isCallExpression(n)) {
                    const pa = isProtectActionCall(n);
                    if (pa.isGate && pa.cbArg) {
                        const nextFull = inFullGate || hasFullGateOptions(pa.optsArg);
                        if (ts.isIdentifier(pa.cbArg) && localFnDefs.has(pa.cbArg.text)) {
                            (nextFull ? fullGatedHelpers : ungatedHelpers).add(pa.cbArg.text);
                        } else {
                            collectHelperCalls(pa.cbArg, nextFull);
                        }
                        return;
                    }
                    if (ts.isIdentifier(n.expression) && localFnDefs.has(n.expression.text)) {
                        (inFullGate ? fullGatedHelpers : ungatedHelpers).add(n.expression.text);
                    }
                }
                ts.forEachChild(n, (child) => collectHelperCalls(child, inFullGate));
            };
            collectHelperCalls(sf, false);

            const fullGatedNodes = new Set<ts.Node>();
            for (const name of fullGatedHelpers) {
                if (!ungatedHelpers.has(name)) {
                    for (const def of localFnDefs.get(name) ?? []) {
                        fullGatedNodes.add(def);
                    }
                }
            }

            const inspect = (
                node: ts.Node,
                inProtectAction: boolean,
                inFullProtectAction: boolean,
            ) => {
                if (fullGatedNodes.has(node)) {
                    ts.forEachChild(node, (child) => inspect(child, true, true));
                    return;
                }

                if (ts.isCallExpression(node)) {
                    const pa = isProtectActionCall(node);
                    if (pa.isGate && pa.cbArg) {
                        const isFull = hasFullGateOptions(pa.optsArg) || inFullProtectAction;

                        const alwaysTerminates = (stmt: ts.Statement): boolean => {
                            if (ts.isReturnStatement(stmt) || ts.isThrowStatement(stmt)) {
                                return true;
                            }
                            if (ts.isBlock(stmt)) {
                                return (
                                    stmt.statements.length > 0 &&
                                    alwaysTerminates(stmt.statements[stmt.statements.length - 1])
                                );
                            }
                            if (ts.isIfStatement(stmt) && stmt.elseStatement) {
                                return (
                                    alwaysTerminates(stmt.thenStatement) &&
                                    alwaysTerminates(stmt.elseStatement)
                                );
                            }
                            return false;
                        };

                        const maxDailyPayPath = (n: ts.Node): string[] => {
                            if (ts.isCallExpression(n)) {
                                const innerPa = isProtectActionCall(n);
                                if (innerPa.isGate) {
                                    inspect(n, true, isFull);
                                    return [];
                                }
                                const k = getCalleeServiceKey(n);
                                if (k === "authService.deleteProfile") {
                                    badDeleteCalls.push(
                                        `${rel}: deleteProfile called inside protectAction`,
                                    );
                                }
                                const here: string[] = [];
                                if (k && serviceToCmd.has(k)) {
                                    const cmd = serviceToCmd.get(k)!;
                                    const tier =
                                        COMMAND_ACCESS_POLICY[
                                            cmd as keyof typeof COMMAND_ACCESS_POLICY
                                        ];
                                    if (tier === "dailyPay") {
                                        here.push(`${k}(${cmd})`);
                                    }
                                    if (
                                        tier === "full" &&
                                        !isFull &&
                                        !EXPLICIT_PASSWORD_REAUTH_COMMANDS.has(
                                            cmd as keyof typeof COMMAND_ACCESS_POLICY,
                                        )
                                    ) {
                                        const line =
                                            sf.getLineAndCharacterOfPosition(n.getStart()).line + 1;
                                        unGatedFullCalls.push(
                                            `${rel}:${line}: ${k} (${cmd}) called inside protectAction without requireFullAccess`,
                                        );
                                    }
                                }
                                let argMax: string[] = [];
                                ts.forEachChild(n, (ch) => {
                                    argMax = argMax.concat(maxDailyPayPath(ch));
                                });
                                return here.concat(argMax);
                            }

                            if (ts.isConditionalExpression(n)) {
                                const condCalls = maxDailyPayPath(n.condition);
                                const a = maxDailyPayPath(n.whenTrue);
                                const b = maxDailyPayPath(n.whenFalse);
                                return condCalls.concat(a.length >= b.length ? a : b);
                            }

                            if (ts.isBlock(n)) {
                                const maxSeq = (idx: number): string[] => {
                                    if (idx >= n.statements.length) return [];
                                    const st = n.statements[idx];
                                    if (ts.isIfStatement(st)) {
                                        const condCalls = maxDailyPayPath(st.expression);
                                        const rest = maxSeq(idx + 1);
                                        const thenTail = alwaysTerminates(st.thenStatement)
                                            ? []
                                            : rest;
                                        const thenCalls = maxDailyPayPath(st.thenStatement).concat(
                                            thenTail,
                                        );
                                        const elseCalls = st.elseStatement
                                            ? maxDailyPayPath(st.elseStatement).concat(
                                                  alwaysTerminates(st.elseStatement) ? [] : rest,
                                              )
                                            : rest;
                                        return condCalls.concat(
                                            thenCalls.length >= elseCalls.length
                                                ? thenCalls
                                                : elseCalls,
                                        );
                                    }
                                    return maxDailyPayPath(st).concat(maxSeq(idx + 1));
                                };
                                return maxSeq(0);
                            }

                            let sum: string[] = [];
                            ts.forEachChild(n, (ch) => {
                                sum = sum.concat(maxDailyPayPath(ch));
                            });
                            return sum;
                        };

                        let worstDailyPayPath: string[] = [];
                        if (ts.isIdentifier(pa.cbArg)) {
                            for (const def of localFnDefs.get(pa.cbArg.text) ?? []) {
                                const p = maxDailyPayPath(def);
                                if (p.length > worstDailyPayPath.length) worstDailyPayPath = p;
                            }
                        } else {
                            worstDailyPayPath = maxDailyPayPath(pa.cbArg);
                        }

                        if (!isFull && worstDailyPayPath.length > 1) {
                            const line = sf.getLineAndCharacterOfPosition(node.getStart()).line + 1;
                            multiDailyPayBlocks.push(
                                `${rel}:${line} has ${worstDailyPayPath.length} dailyPay calls in one non-Full protectAction: [${worstDailyPayPath.join(", ")}]`,
                            );
                        }
                        return;
                    }

                    const key = getCalleeServiceKey(node);
                    if (key === "authService.deleteProfile") {
                        if (!hasExplicitPasswordArgument(node, sf)) {
                            badDeleteCalls.push(
                                `${rel}: deleteProfile called without explicit non-null password`,
                            );
                        }
                    }

                    if (!inProtectAction && key && serviceToCmd.has(key)) {
                        const cmd = serviceToCmd.get(key)!;
                        const tier =
                            COMMAND_ACCESS_POLICY[cmd as keyof typeof COMMAND_ACCESS_POLICY];
                        const isSilentStartupRepair =
                            cmd === "repair_wallet_integrity" &&
                            rel === path.join("hooks", "useIntegrityCheck.ts");
                        if (
                            tier === "full" &&
                            !EXPLICIT_PASSWORD_REAUTH_COMMANDS.has(
                                cmd as keyof typeof COMMAND_ACCESS_POLICY,
                            ) &&
                            !fileHasFullPrecheck &&
                            !hasExplicitPasswordArgument(node, sf) &&
                            !isSilentStartupRepair
                        ) {
                            const line =
                                sf.getLineAndCharacterOfPosition(node.getStart()).line + 1;
                            unGatedFullCalls.push(
                                `${rel}:${line}: ${key} (${cmd}) called outside Full-gated protectAction`,
                            );
                        }
                    }
                }

                ts.forEachChild(node, (child) =>
                    inspect(child, inProtectAction, inFullProtectAction),
                );
            };

            inspect(sf, false, false);
        }

        expect(unGatedFullCalls).toEqual([]);
        expect(multiDailyPayBlocks).toEqual([]);
        expect(badDeleteCalls).toEqual([]);
    });
});
