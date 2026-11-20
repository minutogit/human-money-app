// src/services/__tests__/commandAccessParity.test.ts
import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { COMMAND_ACCESS_POLICY, protectCommandOptions } from "../commandPolicy";

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

describe("Tauri command access policy parity (F3)", () => {
    it("covers every registered Tauri command in src-tauri/src/lib.rs with zero drift", () => {
        const libRs = fs.readFileSync(LIB_RS_PATH, "utf-8");
        const registered = extractRegisteredCommands(libRs).sort();
        const policyKeys = Object.keys(COMMAND_ACCESS_POLICY).sort();

        expect(policyKeys).toEqual(registered);
    });

    it("maps protectCommandOptions to requireFullAccess only for Full-tier commands", () => {
        expect(protectCommandOptions("create_new_voucher")).toEqual({ requireFullAccess: true });
        expect(protectCommandOptions("save_public_sync")).toEqual({ requireFullAccess: true });
        expect(protectCommandOptions("register_guarantor_anchor_role")).toEqual({
            requireFullAccess: true,
        });
        expect(protectCommandOptions("sync_wallet_backup_folder")).toEqual({
            requireFullAccess: true,
        });
        expect(protectCommandOptions("resolve_card_recipient")).toEqual({});
        expect(protectCommandOptions("create_transfer_bundle")).toEqual({});
        expect(protectCommandOptions("dismiss_wallet_sync_alert")).toEqual({});
        expect(protectCommandOptions("get_app_config")).toEqual({});
        expect(protectCommandOptions("list_profiles")).toEqual({});
    });

    it("enforces requireFullAccess or protectCommandOptions across Full-tier UI callers", () => {
        const requiredFullCallers = [
            "components/wallet/CreateVoucher.tsx",
            "components/settings/AutoSyncSummaryCard.tsx",
            "components/settings/sync/useSyncServiceAdmin.ts",
            "components/settings/sync/HostSyncSetupWizard.tsx",
            "components/settings/network/L2LifecycleSection.tsx",
            "components/settings/network/LockServerProbeSection.tsx",
            "components/settings/network/AnchorTemplateBuilder.tsx",
            "components/settings/ReachPresetSelector.tsx",
            "components/settings/IdentityRecoveryCard.tsx",
            "components/settings/SettingsView.tsx",
            "components/settings/BackupSettingsCard.tsx",
            "components/profile/GlobalProfileEditorModal.tsx",
            "components/marketplace/MarketplaceView.tsx",
            "components/wallet/DeadQuorumSwapPanel.tsx",
            "components/wallet/SignRequestView.tsx",
            "components/wallet/IntegrityReportModal.tsx",
            "components/standards/StandardsManager.tsx",
            "components/standards/ImportStandardModal.tsx",
        ];

        for (const relPath of requiredFullCallers) {
            const content = fs.readFileSync(path.join(SRC_ROOT, relPath), "utf-8");
            const usesFullAccessGate =
                content.includes("requireFullAccess: true") ||
                content.includes("protectCommandOptions(");
            expect(
                usesFullAccessGate,
                `${relPath} invokes Full-access backend commands and must request Full-access step-up`,
            ).toBe(true);
        }
    });
});
