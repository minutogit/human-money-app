import { invoke } from "@tauri-apps/api/core";
import { FullProofDetails, ProofOfDoubleSpendSummary } from "../types";

export const integrityService = {
    getProofOfDoubleSpend: async (proofId: string) => {
        return await invoke<FullProofDetails>("get_proof_of_double_spend", { proofId });
    },

    setConflictLocalOverride: async (args: any) => {
        return await invoke<void>("set_conflict_local_override", args);
    },

    checkIntegrity: async () => {
        return await invoke<any>("get_integrity_report");
    },

    getDoubleSpendConflicts: async () => {
        return await invoke<ProofOfDoubleSpendSummary[]>("get_double_spend_conflicts");
    },

    repairWalletIntegrity: async (password: string | undefined) => {
        return await invoke<void>("repair_wallet_integrity", { password });
    }
};


