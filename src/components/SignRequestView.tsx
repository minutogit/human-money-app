// src/components/SignRequestView.tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { logger } from '../utils/log';
import { VoucherDetails, VoucherStandardInfo, AppSettings, SignatureImpact } from '../types';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import { Button } from './ui/Button';
import { useSession } from '../context/SessionContext';

interface SignRequestViewProps {
    voucherData: VoucherDetails;
    onBack: () => void;
}

export function SignRequestView({ voucherData, onBack }: SignRequestViewProps) {
    const { protectAction } = useSession();
    const [allowedRoles, setAllowedRoles] = useState<string[]>([]);
    const [selectedRole, setSelectedRole] = useState<string>('');
    const [includeDetails, setIncludeDetails] = useState(true);
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [isSigning, setIsSigning] = useState(false);
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [standardContent, setStandardContent] = useState<string | null>(null);
    const [impact, setImpact] = useState<SignatureImpact | null>(null);
    const [isImpactLoading, setIsImpactLoading] = useState(false);

    useEffect(() => {
        async function fetchData() {
            try {
                logger.info("SignRequestView opened, fetching standards and roles.");
                const [standards, currentSettings] = await Promise.all([
                    invoke<VoucherStandardInfo[]>("get_voucher_standards"),
                    invoke<AppSettings>('get_app_settings').catch(() => null)
                ]);
                setSettings(currentSettings);

                // Find the standard that matches the voucher's standard UUID
                const matchingStandard = standards.find((s: VoucherStandardInfo) => {
                    const uuidMatch = s.content.match(/uuid\s*=\s*["']([^"']+)["']/);
                    return uuidMatch && uuidMatch[1] === voucherData.voucher.voucher_standard.uuid;
                });

                if (matchingStandard) {
                    setStandardContent(matchingStandard.content);
                    const roles = await invoke<string[]>("get_allowed_signature_roles_from_standard", {
                        tomlContent: matchingStandard.content
                    });
                    setAllowedRoles(roles);
                    if (roles.length === 1) {
                        setSelectedRole(roles[0]);
                    }
                }
            } catch (e) {
                const msg = `Failed to fetch voucher standards, roles or settings: ${e}`;
                logger.error(msg);
                setFeedbackMsg(`Error: ${msg}`);
            }
        }
        fetchData();
    }, [voucherData.voucher.voucher_standard.uuid]);

    useEffect(() => {
        if (!selectedRole || !standardContent) {
            setImpact(null);
            return;
        }

        async function fetchImpact() {
            setIsImpactLoading(true);
            try {
                const impactResult = await invoke<SignatureImpact>("evaluate_signature_suitability", {
                    voucher: voucherData,
                    role: selectedRole,
                    standardTomlContent: standardContent
                });
                setImpact(impactResult);
            } catch (e) {
                logger.error(`Failed to evaluate signature impact: ${e}`);
                setImpact(null);
            } finally {
                setIsImpactLoading(false);
            }
        }
        
        fetchImpact();
    }, [selectedRole, standardContent, voucherData]);

    async function handleSign() {
        if (!selectedRole) {
            setFeedbackMsg("Please select a signature role.");
            return;
        }

        setIsSigning(true);
        setFeedbackMsg('');
        try {
            logger.info(`Creating detached signature response for role ${selectedRole}`);
            const bundleBytes = await protectAction(async (password) => {
                return await invoke<number[]>("create_detached_signature_response_bundle", {
                    voucher: voucherData,
                    role: selectedRole,
                    includeDetails: includeDetails,
                    config: { type: "TargetDid", value: [voucherData.voucher.creator.id!, "TrialDecryption"] },
                    password
                });
            });

            if (!bundleBytes) return;

            const filePath = await save({
                defaultPath: settings?.last_used_directory
                    ? `${settings.last_used_directory}/signature-response-${voucherData.voucher.voucher_id.slice(0, 8)}.sig`
                    : `signature-response-${voucherData.voucher.voucher_id.slice(0, 8)}.sig`,
                filters: [
                    { name: 'Signature Response (.sig)', extensions: ['sig'] },
                    { name: 'All Files', extensions: ['*'] }
                ]
            });

            if (filePath) {
                const uint8Array = new Uint8Array(bundleBytes);
                const { writeFile } = await import("@tauri-apps/plugin-fs");
                await writeFile(filePath, uint8Array);
                logger.info(`Signature response bundle saved to ${filePath}`);
                
                // Save directory for next time
                if (settings) {
                    updateLastUsedDirectory(filePath, settings, protectAction).then(() => {
                        invoke<AppSettings>('get_app_settings').then(setSettings).catch(() => {});
                    });
                }

                setFeedbackMsg("Signature response saved successfully!");
                setTimeout(() => onBack(), 2000);
            }
        } catch (e) {
            const msg = `Failed to create signature response: ${e}`;
            logger.error(msg);
            setFeedbackMsg(msg);
        } finally {
            setIsSigning(false);
        }
    }

    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : 'N/A';

    return (
        <div className="max-w-4xl mx-auto space-y-6 p-4 sm:p-6">
            <header className="flex items-center gap-4">
                <button
                    onClick={onBack}
                    className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                    title="Cancel"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                    </svg>
                </button>
                <h1 className="text-2xl font-bold text-theme-primary">Signature Request</h1>
            </header>

            {feedbackMsg && (
                <div className={`p-4 rounded-lg ${feedbackMsg.includes('Error') ? 'bg-red-100 text-red-800' : 'bg-green-100 text-green-800'}`}>
                    {feedbackMsg}
                </div>
            )}

            <div className="bg-bg-card-alternate border border-theme-subtle rounded-lg shadow-sm p-6">
                <h2 className="text-lg font-semibold text-theme-primary mb-4">Voucher Details</h2>
                <div className="space-y-2">
                    <p><span className="font-semibold">Standard:</span> {voucherData.voucher.voucher_standard.name}</p>
                    <p><span className="font-semibold">Value:</span> {voucherData.voucher.nominal_value.amount} {voucherData.voucher.nominal_value.unit}</p>
                    <p><span className="font-semibold">Creator:</span> {voucherData.voucher.creator.first_name} {voucherData.voucher.creator.last_name}</p>
                    <p><span className="font-semibold">Created:</span> {formatDateTime(voucherData.voucher.creation_date)}</p>
                    <p><span className="font-semibold">Valid Until:</span> {formatDateTime(voucherData.voucher.valid_until)}</p>
                </div>
            </div>

            <div className="bg-bg-card-alternate border border-theme-subtle rounded-lg shadow-sm p-6">
                <h2 className="text-lg font-semibold text-theme-primary mb-4">Signature Options</h2>
                
                <div className="space-y-4">
                    <div>
                        <label className="block text-sm font-semibold text-theme-light mb-2">
                            Signature Role
                        </label>
                        {allowedRoles.length === 0 ? (
                            <p className="text-sm text-theme-light">Loading available roles...</p>
                        ) : allowedRoles.length === 1 ? (
                            <p className="text-base text-theme-secondary">{selectedRole}</p>
                        ) : (
                            <select
                                value={selectedRole}
                                onChange={(e) => setSelectedRole(e.target.value)}
                                className="w-full px-3 py-2 border border-theme-subtle rounded-md bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary"
                            >
                                <option value="">Select a role...</option>
                                {allowedRoles.map(role => (
                                    <option key={role} value={role}>{role}</option>
                                ))}
                            </select>
                        )}
                    </div>

                    <div className="flex items-center gap-2">
                        <input
                            type="checkbox"
                            id="includeDetails"
                            checked={includeDetails}
                            onChange={(e) => setIncludeDetails(e.target.checked)}
                            className="w-4 h-4 text-theme-primary border-theme-subtle rounded focus:ring-theme-primary"
                        />
                        <label htmlFor="includeDetails" className="text-sm text-theme-secondary">
                            Include public profile details in signature
                        </label>
                    </div>
                </div>

                {/* Impact Evaluation UI */}
                <div className="mt-6 pt-4 border-t border-theme-subtle">
                    {isImpactLoading ? (
                        <p className="text-sm text-theme-light italic">Evaluating signature impact...</p>
                    ) : impact ? (
                        <div className="space-y-3">
                            {!impact.is_allowed_role && (
                                <div className="p-3 bg-red-100 text-red-800 rounded-md flex items-start gap-2">
                                    <span className="text-lg">🛑</span>
                                    <p className="text-sm">The standard does not permit this role.</p>
                                </div>
                            )}
                            
                            {impact.fatal_conflicts.length > 0 && (
                                <div className="p-3 bg-red-100 text-red-800 rounded-md flex items-start gap-2">
                                    <span className="text-lg">🚫</span>
                                    <div>
                                        <p className="text-sm font-semibold mb-1">Warning: Profile conflict</p>
                                        <ul className="list-disc pl-4 text-sm space-y-1">
                                            {impact.fatal_conflicts.map((conflict, i) => (
                                                <li key={i}>{conflict}</li>
                                            ))}
                                        </ul>
                                    </div>
                                </div>
                            )}

                            {impact.resolved_rules.length > 0 && (
                                <div className="p-3 bg-green-100 text-green-800 rounded-md flex items-start gap-2">
                                    <span className="text-lg">🌟</span>
                                    <div>
                                        <p className="text-sm font-semibold mb-1">Your signature will successfully fulfill:</p>
                                        <ul className="list-disc pl-4 text-sm space-y-1">
                                            {impact.resolved_rules.map((rule, i) => (
                                                <li key={i}>{rule}</li>
                                            ))}
                                        </ul>
                                    </div>
                                </div>
                            )}

                            {impact.gentle_hints.length > 0 && impact.fatal_conflicts.length === 0 && (
                                <div className="p-3 bg-blue-50 text-blue-800 border border-blue-200 rounded-md flex flex-col gap-2">
                                    {impact.gentle_hints.map((hint, i) => (
                                        <div key={i} className="flex items-start gap-2">
                                            <span className="text-lg">💡</span>
                                            <p className="text-sm">{hint}</p>
                                        </div>
                                    ))}
                                </div>
                            )}

                            {impact.is_allowed_role && impact.fatal_conflicts.length === 0 && impact.resolved_rules.length === 0 && impact.gentle_hints.length === 0 && (
                                <div className="p-3 bg-gray-100 text-gray-700 rounded-md flex items-start gap-2">
                                    <span className="text-lg">⚠️</span>
                                    <p className="text-sm">This signature does not directly resolve any pending rules.</p>
                                </div>
                            )}
                        </div>
                    ) : selectedRole ? (
                        <p className="text-sm text-theme-light italic">No evaluation data available.</p>
                    ) : null}
                </div>
            </div>

            <Button
                size="lg"
                onClick={handleSign}
                disabled={!selectedRole || isSigning || (impact !== null && (!impact.is_allowed_role || impact.fatal_conflicts.length > 0))}
                className="w-full"
            >
                {isSigning ? 'Signing...' : 'Accept & Sign'}
            </Button>
        </div>
    );
}
