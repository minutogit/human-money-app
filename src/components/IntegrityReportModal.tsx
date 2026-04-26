import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { IntegrityReport } from "../types";
import { useSession } from "../context/SessionContext";
import { logger } from "../utils/log";

interface IntegrityReportModalProps {
    report: IntegrityReport;
    onClose: () => void;
}

export function IntegrityReportModal({ report, onClose }: IntegrityReportModalProps) {
    const { protectAction, checkIntegrity } = useSession();
    const [isRepairing, setIsRepairing] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const handleRepair = async () => {
        setIsRepairing(true);
        setError(null);
        try {
            await protectAction(async (password) => {
                await invoke("repair_wallet_integrity", { password });
            });
            logger.info("Wallet integrity repaired successfully.");
            await checkIntegrity();
            onClose();
        } catch (e) {
            setError(String(e));
            logger.error(`Failed to repair integrity: ${e}`);
        } finally {
            setIsRepairing(false);
        }
    };

    const getReportDetails = () => {
        switch (report.type) {
            case 'MissingItems':
                return {
                    title: "Missing Items",
                    description: "Some critical wallet data items are missing from the disk. This could lead to data loss.",
                    severity: "error",
                    items: report.items
                };
            case 'ManipulatedItems':
                return {
                    title: "Manipulated Items",
                    description: "Data items have been modified outside of this application. This is a security risk.",
                    severity: "error",
                    items: report.items
                };
            case 'UnknownItems':
                return {
                    title: "Unknown Items",
                    description: "Unrecognized data items were found in your wallet directory.",
                    severity: "warning",
                    items: report.items
                };
            case 'IntegrityOutdated':
                return {
                    title: "Outdated Integrity Record",
                    description: "The security integrity record does not match the current wallet state epoch. Possible rollback attempt.",
                    severity: "error"
                };
            case 'InvalidSignature':
                return {
                    title: "Invalid Signature",
                    description: "The security integrity record signature is invalid. The record itself might have been tampered with.",
                    severity: "error"
                };
            case 'MissingIntegrityRecord':
                return {
                    title: "Missing Integrity Record",
                    description: "Your wallet does not have a security integrity record yet. You should create one to enable protection.",
                    severity: "info"
                };
            default:
                return null;
        }
    };

    const details = getReportDetails();
    if (!details) return null;

    return (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 backdrop-blur-md p-4 animate-in fade-in duration-300">
            <div className="max-w-2xl w-full bg-white dark:bg-gray-900 rounded-3xl shadow-2xl overflow-hidden border border-white/10 flex flex-col max-h-[90vh]">
                <div className={`p-8 ${details.severity === 'error' ? 'bg-red-500/10' : details.severity === 'warning' ? 'bg-amber-500/10' : 'bg-blue-500/10'}`}>
                    <div className="flex items-center gap-4 mb-2">
                        <div className={`p-3 rounded-2xl ${details.severity === 'error' ? 'bg-red-500 text-white' : details.severity === 'warning' ? 'bg-amber-500 text-white' : 'bg-blue-500 text-white'}`}>
                            {details.severity === 'error' ? (
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                                </svg>
                            ) : (
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                                </svg>
                            )}
                        </div>
                        <div>
                            <h2 className="text-2xl font-bold text-gray-900 dark:text-white">{details.title}</h2>
                            <p className="text-sm font-medium uppercase tracking-wider opacity-60">Storage Integrity Report</p>
                        </div>
                    </div>
                </div>

                <div className="p-8 overflow-y-auto">
                    <p className="text-lg text-gray-600 dark:text-gray-300 mb-6">
                        {details.description}
                    </p>

                    {details.items && details.items.length > 0 && (
                        <div className="mb-8">
                            <h3 className="text-sm font-bold text-gray-400 uppercase tracking-widest mb-3">Affected Items</h3>
                            <div className="bg-gray-50 dark:bg-gray-800/50 rounded-2xl p-4 border border-gray-100 dark:border-gray-800 max-h-48 overflow-y-auto font-mono text-sm">
                                {details.items.map((item, i) => (
                                    <div key={i} className="py-1 flex items-center gap-2">
                                        <span className="opacity-30">•</span>
                                        <span className="text-gray-700 dark:text-gray-200">{item}</span>
                                    </div>
                                ))}
                            </div>
                        </div>
                    )}

                    {error && (
                        <div className="mb-6 p-4 bg-red-500/10 border border-red-500/20 rounded-2xl text-red-600 dark:text-red-400 text-sm flex items-start gap-3">
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                            {error}
                        </div>
                    )}

                    <div className="bg-blue-500/5 rounded-2xl p-4 border border-blue-500/10 text-blue-600 dark:text-blue-400 text-sm italic">
                        By repairing the wallet, you accept the current state of items as valid and sign a new security integrity record.
                    </div>
                </div>

                <div className="p-8 bg-gray-50 dark:bg-gray-800/30 border-t border-gray-100 dark:border-gray-800 flex gap-4">
                    <button
                        onClick={onClose}
                        className="flex-1 px-6 py-4 rounded-2xl font-bold text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                    >
                        Close
                    </button>
                    <button
                        onClick={handleRepair}
                        disabled={isRepairing}
                        className={`flex-[2] px-6 py-4 rounded-2xl font-bold text-white shadow-lg shadow-blue-500/20 transition-all ${isRepairing ? 'bg-blue-400 cursor-not-allowed' : 'bg-blue-500 hover:bg-blue-600 hover:-translate-y-0.5 active:translate-y-0'}`}
                    >
                        {isRepairing ? (
                            <span className="flex items-center justify-center gap-2">
                                <svg className="animate-spin h-5 w-5 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                                </svg>
                                Repairing...
                            </span>
                        ) : "Repair Integrity Record & Accept State"}
                    </button>
                </div>
            </div>
        </div>
    );
}
