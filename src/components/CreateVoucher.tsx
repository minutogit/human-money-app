import { useState, FormEvent, useRef } from "react";
import { voucherService } from "../services/voucherService";
import { Button } from "./ui/Button";
import { useSession } from "../context/SessionContext";
import { ConfirmationModal } from "./ui/ConfirmationModal";
import { PageLayout } from "./ui/PageLayout";
import { 
    ShieldAlert, 
    Info, 
    CheckCircle2,
    Clock,
    AlertCircle,
    Lock, 
    PlusCircle
} from "lucide-react";

import { useVoucherCreation } from "../hooks/useVoucherCreation";
import { VoucherBasicsForm } from "./voucher/VoucherBasicsForm";
import { CreatorIdentityForm } from "./voucher/CreatorIdentityForm";
import { CollateralForm } from "./voucher/CollateralForm";

interface CreateVoucherProps {
    onVoucherCreated: () => void;
    onCancel: () => void;
}

export function CreateVoucher({ onVoucherCreated, onCancel }: CreateVoucherProps) {
    const { protectAction } = useSession();
    const {
        standards,
        selectedStandardId,
        parsedStandard,
        amount,
        validityValue,
        validityUnit,
        nonRedeemable,
        identity,
        collateral,
        isLoading,
        coordWarning,
        errors,
        setAmount,
        setValidityValue,
        setValidityUnit,
        setNonRedeemable,
        setIdentity,
        setCollateral,
        handleStandardChange,
        handleLoadProfile,
        handleCoordBlur,
        validate,
        prepareVoucherData,
        setIsLoading
    } = useVoucherCreation();

    const [feedback, setFeedback] = useState<{ type: 'error' | 'success', msg: string } | null>(null);
    const [showConfirm, setShowConfirm] = useState(false);
    const [showTestVoucherWarning, setShowTestVoucherWarning] = useState(false);

    // Refs für Fokus-Management
    const standardRef = useRef<HTMLSelectElement>(null);
    const amountRef = useRef<HTMLInputElement>(null);
    const firstNameRef = useRef<HTMLInputElement>(null);
    const lastNameRef = useRef<HTMLInputElement>(null);

    const handleCreateClick = (event: FormEvent) => {
        event.preventDefault();
        if (isLoading) return;
        
        const currentErrors = validate();
        if (Object.keys(currentErrors).length > 0) {
            // Fokus auf das erste fehlerhafte Feld setzen und dorthin scrollen
            if (currentErrors.standard) {
                standardRef.current?.focus();
                standardRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            } else if (currentErrors.amount) {
                amountRef.current?.focus();
                amountRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            } else if (currentErrors.firstName) {
                firstNameRef.current?.focus();
                firstNameRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            } else if (currentErrors.lastName) {
                lastNameRef.current?.focus();
                lastNameRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
            }
            
            setFeedback({ type: 'error', msg: "Please fill in all required fields." });
            return;
        }

        setShowConfirm(true);
    };

    async function executeCreation() {
        setIsLoading(true);
        const selectedStandard = standards.find(s => s.id === selectedStandardId)!;
        const voucherData = prepareVoucherData();
        
        try {
            await protectAction(async (password) => {
                await voucherService.create(selectedStandard.content, voucherData, password || undefined);
            });
            setFeedback({type: 'success', msg: "Voucher created! Synchronizing ledger..."});
            setTimeout(onVoucherCreated, 2000);
        } catch (e) {
            setFeedback({type: 'error', msg: `Creation Failed: ${e}`});
            setIsLoading(false);
            setShowConfirm(false);
        }
    }

    return (
        <PageLayout 
            title="Create Voucher" 
            description="Create a new voucher." 
            onBack={onCancel}
        >
            <div className="max-w-5xl mx-auto space-y-8 pb-20">
                {feedback && (
                    <div className={`p-5 rounded-3xl flex items-start gap-3 border shadow-sm animate-in zoom-in duration-300 ${feedback.type === 'error' ? 'bg-rose-50 border-rose-100 text-rose-800' : 'bg-emerald-50 border-emerald-100 text-emerald-800'}`}>
                        {feedback.type === 'error' ? <ShieldAlert className="shrink-0" size={20}/> : <CheckCircle2 className="shrink-0" size={20}/>}
                        <p className="text-sm font-bold leading-tight">{feedback.msg}</p>
                    </div>
                )}

                <form onSubmit={handleCreateClick} className="space-y-8">
                    <VoucherBasicsForm 
                        standards={standards}
                        selectedStandardId={selectedStandardId}
                        onStandardChange={handleStandardChange}
                        amount={amount}
                        onAmountChange={setAmount}
                        validityValue={validityValue}
                        onValidityValueChange={setValidityValue}
                        validityUnit={validityUnit}
                        onValidityUnitChange={setValidityUnit}
                        nonRedeemable={nonRedeemable}
                        onNonRedeemableChange={(val) => {
                            if (val) setNonRedeemable(true);
                            else setShowTestVoucherWarning(true);
                        }}
                        isLoading={isLoading}
                        errors={errors}
                        standardRef={standardRef}
                        amountRef={amountRef}
                    />

                    <CreatorIdentityForm 
                        identity={identity}
                        onIdentityChange={setIdentity}
                        onLoadProfile={handleLoadProfile}
                        onCoordBlur={handleCoordBlur}
                        coordWarning={coordWarning}
                        errors={errors}
                        firstNameRef={firstNameRef}
                        lastNameRef={lastNameRef}
                    />

                    <CollateralForm 
                        collateral={collateral}
                        onCollateralChange={setCollateral}
                    />

                    <div className="bg-amber-500/10 border border-amber-500/20 p-6 rounded-[32px] flex items-start gap-5 shadow-sm">
                        <div className="p-3 bg-white rounded-2xl shadow-sm text-amber-600">
                            <Info size={24} />
                        </div>
                        <div>
                            <h3 className="text-sm font-black text-amber-900 uppercase tracking-widest mb-1">Public Data Notice</h3>
                            <p className="text-xs text-amber-800/80 font-medium leading-relaxed">
                                The information embedded in this voucher serves as your cryptographic guarantee. All creator details will be permanently readable by anyone verifying this asset on the public ledger.
                            </p>
                        </div>
                    </div>

                    <div className="flex flex-col items-center gap-4 py-4">
                        {Object.keys(errors).length > 0 && (
                            <div className="flex items-center gap-2 text-rose-600 bg-rose-50 px-4 py-2 rounded-xl border border-rose-100 animate-in fade-in slide-in-from-bottom-2 duration-300">
                                <AlertCircle size={16} />
                                <span className="text-xs font-bold">Please fill in all required fields.</span>
                            </div>
                        )}
                        <Button type="submit" disabled={isLoading} className="min-w-[320px] py-5 rounded-3xl shadow-premium-lg text-lg gap-3">
                            {isLoading ? <Clock className="animate-spin" size={24} /> : <PlusCircle size={24} />}
                            {isLoading ? "Creating..." : "Create Voucher"}
                        </Button>
                            <p className="text-[10px] font-bold text-theme-light flex items-center gap-2">
                                <Lock size={12} /> You will sign this voucher in the next step.
                            </p>
                    </div>
                </form>
            </div>

            <ConfirmationModal
                isOpen={showConfirm}
                title="Create Voucher?"
                description={
                    <div className="space-y-4 pt-2">
                        <div className="p-6 bg-theme-primary/5 rounded-[32px] border border-theme-primary/20 text-center">
                            <p className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light mb-2">New Voucher</p>
                            <p className="text-3xl font-black text-theme-primary tracking-tighter">
                                {(() => {
                                    if (!parsedStandard) return amount;
                                    const abbreviation = parsedStandard.immutable.identity.abbreviation;
                                    const unit = parsedStandard.immutable.blueprint.unit;
                                    let displayUnit = abbreviation || unit || 'Units';
                                    if (nonRedeemable && !displayUnit.startsWith("TEST-")) displayUnit = `TEST-${displayUnit}`;
                                    return `${amount} ${displayUnit}`;
                                })()}
                            </p>
                        </div>
                        <p className="text-sm font-medium text-theme-secondary leading-relaxed">
                            This action will cryptographically sign the asset with your private key. The voucher will be permanently attributed to your identity. Proceed?
                        </p>
                    </div>
                }
                confirmText="Create Voucher"
                onConfirm={executeCreation}
                onCancel={() => setShowConfirm(false)}
                isProcessing={isLoading}
            />

            <ConfirmationModal
                isOpen={showTestVoucherWarning}
                title="Warning"
                confirmVariant="danger"
                description={
                    <div className="space-y-4 pt-2">
                        <div className="p-5 bg-rose-50 border border-rose-100 rounded-3xl flex items-start gap-4">
                            <ShieldAlert size={24} className="text-rose-500 mt-1 shrink-0" />
                            <p className="text-sm font-medium text-rose-900 leading-relaxed">
                                You are about to issue a <strong>real redeemable voucher</strong>. In this early development phase, we recommend using Test Assets to prevent potential asset loss.
                            </p>
                        </div>
                        <p className="text-sm font-bold text-theme-secondary">Proceed with real cryptographic obligation?</p>
                    </div>
                }
                confirmText="Create Real Voucher"
                cancelText="Back to Safety"
                onConfirm={() => { setNonRedeemable(false); setShowTestVoucherWarning(false); }}
                onCancel={() => { setNonRedeemable(true); setShowTestVoucherWarning(false); }}
            />
        </PageLayout>
    );
}
