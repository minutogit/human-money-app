// src/components/SignRequestView.tsx
import { useState, useEffect } from 'react';
import { voucherService } from '../services/voucherService';
import { settingsService } from '../services/settingsService';
import { standardsService } from '../services/standardsService';
import { save } from '@tauri-apps/plugin-dialog';
import { logger } from '../utils/log';
import { VoucherDetails, VoucherStandardInfo, AppSettings, SignatureImpact } from '../types';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import { Button } from './ui/Button';
import { Card } from './ui/Card';
import { PageLayout } from './ui/PageLayout';
import { useSession } from '../context/SessionContext';
import { 
    PenTool, 
    ShieldCheck, 
    ShieldAlert, 
    Info, 
    Lightbulb, 
    CheckCircle2, 
    XCircle,
    User,
    Calendar,
    UserCheck,
    FileText,
    ArrowRight,
    Lock
} from 'lucide-react';

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
                    standardsService.getStandards(),
                    settingsService.getSettings().catch(() => null)
                ]);
                setSettings(currentSettings);

                const matchingStandard = standards.find((s: VoucherStandardInfo) => {
                    const uuidMatch = s.content.match(/uuid\s*=\s*["']([^"']+)["']/);
                    return uuidMatch && uuidMatch[1] === voucherData.voucher.voucherStandard.uuid;
                });

                if (matchingStandard) {
                    setStandardContent(matchingStandard.content);
                    try {
                        const parsed = await standardsService.parseStandard(matchingStandard.content);
                        
                        // Extract roles from signatureRules keys
                        const roles = Object.keys(parsed.immutable.signatureRules);
                        setAllowedRoles(roles);
                        if (roles.length === 1) setSelectedRole(roles[0]);
                    } catch (e) {
                        setFeedbackMsg(`Failed to parse standard: ${e}`);
                    }
                }
            } catch (e) {
                setFeedbackMsg(`Initialization Error: ${e}`);
            }
        }
        fetchData();
    }, [voucherData.voucher.voucherStandard.uuid]);

    useEffect(() => {
        if (!selectedRole || !standardContent) {
            setImpact(null);
            return;
        }

        async function fetchImpact() {
            setIsImpactLoading(true);
            try {
                const impactResult = await standardsService.evaluateSignatureSuitability({
                    voucher: voucherData.voucher,
                    role: selectedRole,
                    standardTomlContent: standardContent as string
                });
                setImpact(impactResult);
            } catch (e) {
                logger.error(`Failed to evaluate impact: ${e}`);
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
            const bundleBytes = await protectAction(async (password) => {
                return await voucherService.createDetachedSignatureResponseBundle({
                    voucher: voucherData.voucher,
                    role: selectedRole,
                    includeDetails: includeDetails,
                    config: { type: "TargetDid", value: [voucherData.voucher.creator.id!, "TrialDecryption"] },
                    password: password || undefined
                });
            });

            if (!bundleBytes) return;

            const filePath = await save({
                defaultPath: settings?.lastUsedDirectory
                    ? `${settings.lastUsedDirectory}/signature-${voucherData.voucher.voucherId.slice(0, 8)}.sig`
                    : `signature-${voucherData.voucher.voucherId.slice(0, 8)}.sig`,
                filters: [
                    { name: 'Signature Response (.sig)', extensions: ['sig'] },
                    { name: 'All Files', extensions: ['*'] }
                ]
            });

            if (filePath) {
                const uint8Array = new Uint8Array(bundleBytes);
                const { writeFile } = await import("@tauri-apps/plugin-fs");
                await writeFile(filePath, uint8Array);
                
                if (settings) {
                    updateLastUsedDirectory(filePath, settings, protectAction).then(() => {
                        settingsService.getSettings().then(setSettings).catch(() => {});
                    });
                }

                setFeedbackMsg("Signature generated successfully!");
                setTimeout(() => onBack(), 2000);
            }
        } catch (e) {
            setFeedbackMsg(`Signing Failed: ${e}`);
        } finally {
            setIsSigning(false);
        }
    }

    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : 'N/A';

    return (
        <PageLayout 
            title="Signature Request" 
            description="Review and cryptographically sign the requested asset endorsement." 
            onBack={onBack}
        >
            <div className="max-w-4xl mx-auto space-y-8 pb-10">
                {feedbackMsg && (
                    <div className={`p-5 rounded-3xl flex items-start gap-3 border shadow-sm animate-in zoom-in duration-300 ${feedbackMsg.includes('Error') || feedbackMsg.includes('Failed') ? 'bg-rose-50 border-rose-100 text-rose-800' : 'bg-emerald-50 border-emerald-100 text-emerald-800'}`}>
                        {feedbackMsg.includes('Failed') || feedbackMsg.includes('Error') ? <ShieldAlert className="shrink-0" size={20}/> : <CheckCircle2 className="shrink-0" size={20}/>}
                        <p className="text-sm font-bold leading-tight">{feedbackMsg}</p>
                    </div>
                )}

                <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
                    {/* Voucher Summary */}
                    <Card header={
                        <div className="flex items-center gap-2">
                            <FileText size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest text-theme-primary">Voucher Details</span>
                        </div>
                    }>
                        <div className="space-y-4">
                            <div className="p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/10">
                                <div className="flex items-center justify-between mb-1">
                                    <span className="text-[10px] font-black uppercase tracking-widest text-theme-light">Value</span>
                                    <div className="bg-emerald-500 text-white px-2 py-0.5 rounded text-[9px] font-black uppercase tracking-widest">Valid Asset</div>
                                </div>
                                <p className="text-2xl font-black text-theme-primary tracking-tight">
                                    {voucherData.voucher.nominalValue.amount} <span className="text-theme-secondary opacity-60 font-medium">{voucherData.displayCurrency}</span>
                                </p>
                            </div>
                            
                            <div className="space-y-3 px-1">
                                <div className="flex items-center justify-between text-xs font-medium">
                                    <span className="text-theme-light flex items-center gap-1.5"><ShieldCheck size={14}/> Standard</span>
                                    <span className="text-theme-secondary font-bold">{voucherData.displayStandardName}</span>
                                </div>
                                <div className="flex items-center justify-between text-xs font-medium">
                                    <span className="text-theme-light flex items-center gap-1.5"><User size={14}/> Creator</span>
                                    <span className="text-theme-secondary font-bold">{voucherData.voucher.creator.firstName} {voucherData.voucher.creator.lastName}</span>
                                </div>
                                <div className="flex items-center justify-between text-xs font-medium">
                                    <span className="text-theme-light flex items-center gap-1.5"><Calendar size={14}/> Created</span>
                                    <span className="text-theme-secondary font-bold">{formatDateTime(voucherData.voucher.creationDate)}</span>
                                </div>
                            </div>
                        </div>
                    </Card>

                    {/* Role Selection */}
                    <Card header={
                        <div className="flex items-center gap-2">
                            <UserCheck size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest text-theme-primary">Signature Role</span>
                        </div>
                    }>
                        <div className="space-y-6">
                            <div className="space-y-3">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Select Authority Role</label>
                                {allowedRoles.length === 0 ? (
                                    <div className="py-2 animate-pulse flex items-center gap-2">
                                        <div className="h-4 w-4 bg-theme-subtle rounded-full"></div>
                                        <div className="h-4 w-32 bg-theme-subtle rounded-full"></div>
                                    </div>
                                ) : allowedRoles.length === 1 ? (
                                    <div className="p-4 bg-white border-2 border-theme-primary rounded-2xl flex items-center justify-between shadow-sm">
                                        <span className="font-black text-theme-primary tracking-tight">{selectedRole}</span>
                                        <CheckCircle2 size={18} className="text-theme-primary" />
                                    </div>
                                ) : (
                                    <select
                                        value={selectedRole}
                                        onChange={(e) => setSelectedRole(e.target.value)}
                                        className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all"
                                    >
                                        <option value="">Choose Role...</option>
                                        {allowedRoles.map(role => <option key={role} value={role}>{role}</option>)}
                                    </select>
                                )}
                            </div>

                            <label className="flex items-center gap-4 p-4 bg-white/40 border border-theme-subtle rounded-2xl cursor-pointer group hover:border-theme-primary/40 transition-all shadow-sm">
                                <input
                                    type="checkbox"
                                    checked={includeDetails}
                                    onChange={(e) => setIncludeDetails(e.target.checked)}
                                    className="w-6 h-6 rounded-lg border-theme-subtle text-theme-primary focus:ring-theme-primary transition-all"
                                />
                                <div>
                                    <span className="block text-sm font-bold text-theme-secondary group-hover:text-theme-primary transition-colors">Disclose Identity</span>
                                    <span className="block text-[10px] text-theme-light font-medium">Embed your public profile data in the signature</span>
                                </div>
                            </label>
                        </div>
                    </Card>
                </div>

                {/* Impact Analysis */}
                <Card header={
                    <div className="flex items-center gap-2">
                        <PenTool size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest text-theme-primary">Impact Evaluation</span>
                    </div>
                }>
                    <div className="min-h-[120px] flex flex-col justify-center">
                        {isImpactLoading ? (
                            <div className="flex flex-col items-center gap-3 animate-pulse">
                                <div className="w-10 h-10 bg-theme-subtle/20 rounded-full border border-theme-subtle/30 flex items-center justify-center">
                                    <ArrowRight className="text-theme-light animate-bounce" size={20} />
                                </div>
                                <p className="text-[10px] font-black uppercase tracking-widest text-theme-light">Checking signature validity...</p>
                            </div>
                        ) : impact ? (
                            <div className="space-y-4">
                                {!impact.isAllowedRole && (
                                    <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-start gap-3">
                                        <XCircle size={20} className="text-rose-500 shrink-0" />
                                        <p className="text-sm font-bold text-rose-800">Standard Violation: The specified role is not authorized for this asset.</p>
                                    </div>
                                )}
                                
                                {impact.fatalConflicts.length > 0 && (
                                    <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl space-y-2">
                                        <div className="flex items-center gap-2 text-rose-600 font-black text-[10px] uppercase tracking-widest">
                                            <ShieldAlert size={14} /> Profile Conflicts
                                        </div>
                                        <ul className="space-y-1.5">
                                            {impact.fatalConflicts.map((conflict, i) => (
                                                <li key={i} className="text-sm font-bold text-rose-800 flex items-start gap-2">
                                                    <span className="text-rose-400 mt-1.5 flex-shrink-0 w-1 h-1 bg-rose-400 rounded-full"></span>
                                                    {conflict}
                                                </li>
                                            ))}
                                        </ul>
                                    </div>
                                )}

                                {impact.resolvedRules.length > 0 && (
                                    <div className="p-4 bg-emerald-50 border border-emerald-100 rounded-2xl space-y-2">
                                        <div className="flex items-center gap-2 text-emerald-600 font-black text-[10px] uppercase tracking-widest">
                                            <CheckCircle2 size={14} /> Rules Fulfilled
                                        </div>
                                        <ul className="space-y-1.5">
                                            {impact.resolvedRules.map((rule, i) => (
                                                <li key={i} className="text-sm font-bold text-emerald-800 flex items-start gap-2">
                                                    <span className="text-emerald-400 mt-1.5 flex-shrink-0 w-1 h-1 bg-emerald-400 rounded-full"></span>
                                                    {rule}
                                                </li>
                                            ))}
                                        </ul>
                                    </div>
                                )}

                                {impact.gentleHints.length > 0 && impact.fatalConflicts.length === 0 && (
                                    <div className="space-y-2">
                                        {impact.gentleHints.map((hint, i) => (
                                            <div key={i} className="p-4 bg-blue-50/50 border border-blue-100 rounded-2xl flex items-start gap-3">
                                                <Lightbulb size={20} className="text-blue-500 shrink-0" />
                                                <p className="text-sm font-bold text-blue-800">{hint}</p>
                                            </div>
                                        ))}
                                    </div>
                                )}

                                {impact.isAllowedRole && impact.fatalConflicts.length === 0 && impact.resolvedRules.length === 0 && impact.gentleHints.length === 0 && (
                                    <div className="p-4 bg-gray-50 border border-gray-100 rounded-2xl flex items-start gap-3">
                                        <Info size={20} className="text-gray-400 shrink-0" />
                                        <p className="text-sm font-bold text-gray-600">This signature is valid and ready for use.</p>
                                    </div>
                                )}
                            </div>
                        ) : selectedRole ? (
                            <div className="text-center py-4">
                                <p className="text-sm font-bold text-theme-light italic">Impact data unavailable for this role configuration.</p>
                            </div>
                        ) : (
                            <div className="text-center py-4">
                                <p className="text-sm font-bold text-theme-light/60 italic">Please select a role to evaluate the endorsement impact.</p>
                            </div>
                        )}
                    </div>
                </Card>

                <div className="flex flex-col gap-4">
                    <Button
                        size="lg"
                        onClick={handleSign}
                        disabled={!selectedRole || isSigning || (impact !== null && (!impact.isAllowedRole || impact.fatalConflicts.length > 0))}
                        className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3 disabled:opacity-30 disabled:grayscale transition-all"
                    >
                        {isSigning ? <ShieldCheck className="animate-pulse" size={24} /> : <PenTool size={24} />}
                        {isSigning ? 'Signing...' : 'Sign Now'}
                    </Button>
                    <p className="text-[10px] font-bold text-theme-light text-center flex items-center justify-center gap-2">
                        <Lock size={12} />
                        SIGNATURE IS GENERATED LOCALLY AND NEVER EXPOSES YOUR PRIVATE KEY
                    </p>
                </div>
            </div>
        </PageLayout>
    );
}
