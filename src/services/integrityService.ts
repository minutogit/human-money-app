import { invoke } from "@tauri-apps/api/core";
import { FullProofDetails, ProofOfDoubleSpendSummary, ProofImportResult, IntegrityReport } from "../types";
import { APP_EVENTS, emitAppEvent } from "../utils/appEvents";

export interface ConflictOverrideArgs {
    proofId: string;
    isOverridden: boolean;
    resolutionNote?: string;
    password?: string;
}

export interface EndorseConflictArgs {
    proofId: string;
    notes?: string;
    alsoSetLocalOverride: boolean;
    password?: string;
}

export const integrityService = {
    getProofOfDoubleSpend: async (proofId: string) => {
        return await invoke<FullProofDetails>("get_proof_of_double_spend", { proofId });
    },

    setConflictLocalOverride: async (args: ConflictOverrideArgs) => {
        const res = await invoke<void>("set_conflict_local_override", {
            proofId: args.proofId,
            value: args.isOverridden,
            note: args.resolutionNote,
            password: args.password
        });
        emitAppEvent(APP_EVENTS.CONFLICTS_UPDATED);
        return res;
    },

    endorseConflictResolution: async (args: EndorseConflictArgs) => {
        const res = await invoke<FullProofDetails>("endorse_conflict_resolution", {
            proofId: args.proofId,
            notes: args.notes,
            alsoSetLocalOverride: args.alsoSetLocalOverride,
            password: args.password
        });
        emitAppEvent(APP_EVENTS.CONFLICTS_UPDATED);
        return res;
    },

    checkIntegrity: async () => {
        return await invoke<IntegrityReport>("check_wallet_integrity");
    },

    getDoubleSpendConflicts: async () => {
        return await invoke<ProofOfDoubleSpendSummary[]>("get_double_spend_conflicts");
    },

    repairWalletIntegrity: async (password: string | undefined) => {
        const res = await invoke<void>("repair_wallet_integrity", { password });
        emitAppEvent(APP_EVENTS.CONFLICTS_UPDATED);
        return res;
    },

    exportDoubleSpendProof: async (proofId: string) => {
        return await invoke<number[]>("export_double_spend_proof", { proofId });
    },

    importDoubleSpendProof: async (proofBytes: number[], password?: string) => {
        const res = await invoke<ProofImportResult>("import_double_spend_proof", { proofBytes, password: password || null });
        emitAppEvent(APP_EVENTS.CONFLICTS_UPDATED);
        return res;
    }
};



