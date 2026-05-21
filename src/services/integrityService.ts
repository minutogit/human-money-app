import { invoke } from "@tauri-apps/api/core";
import { FullProofDetails, ProofOfDoubleSpendSummary, IntegrityReport } from "../types";

export interface ConflictOverrideArgs {
    proofId: string;
    isOverridden: boolean;
    resolutionNote?: string;
    password?: string;
}

export const integrityService = {
    getProofOfDoubleSpend: async (proofId: string) => {
        return await invoke<FullProofDetails>("get_proof_of_double_spend", { proofId });
    },

    setConflictLocalOverride: async (args: ConflictOverrideArgs) => {
        return await invoke<void>("set_conflict_local_override", {
            proof_id: args.proofId,
            is_overridden: args.isOverridden,
            resolution_note: args.resolutionNote,
            password: args.password
        });
    },

    checkIntegrity: async () => {
        return await invoke<IntegrityReport>("check_wallet_integrity");
    },

    getDoubleSpendConflicts: async () => {
        return await invoke<ProofOfDoubleSpendSummary[]>("get_double_spend_conflicts");
    },

    repairWalletIntegrity: async (password: string | undefined) => {
        return await invoke<void>("repair_wallet_integrity", { password });
    }
};


