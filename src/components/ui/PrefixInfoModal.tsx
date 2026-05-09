import { Button } from './Button';
import { HelpCircle, AlertTriangle, Shield, ShieldAlert } from 'lucide-react';

interface PrefixInfoModalProps {
    isOpen: boolean;
    onClose: () => void;
}

export function PrefixInfoModal({ isOpen, onClose }: PrefixInfoModalProps) {
    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md p-4 animate-in fade-in duration-300">
            <div className="w-full max-w-xl bg-white border border-theme-subtle rounded-[32px] overflow-hidden shadow-premium-2xl animate-in zoom-in-95 slide-in-from-bottom-10 duration-500">
                {/* Header */}
                <div className="bg-theme-primary/5 p-8 border-b border-theme-subtle">
                    <div className="flex items-center gap-4 mb-2">
                        <div className="p-3 bg-theme-primary/10 rounded-2xl text-theme-primary">
                            <HelpCircle size={28} />
                        </div>
                        <h2 className="text-2xl font-black text-theme-primary tracking-tight">Understanding Device Prefixes</h2>
                    </div>
                    <p className="text-theme-light font-medium">Why they matter and how to use them safely.</p>
                </div>

                {/* Content */}
                <div className="p-8 space-y-8 max-h-[60vh] overflow-y-auto custom-scrollbar">
                    {/* Section 1: Concept */}
                    <div className="space-y-3">
                        <div className="flex items-center gap-2 text-theme-secondary">
                            <Shield size={18} className="text-theme-primary" />
                            <h3 className="font-black text-sm uppercase tracking-widest">The Concept of Device Prefixes</h3>
                        </div>
                        <div className="space-y-4 text-theme-secondary leading-relaxed">
                            <p>
                                When you create a wallet, the prefix is attached to your DID key during transfers 
                                (e.g., <code className="bg-theme-primary/5 px-1.5 py-0.5 rounded font-bold text-theme-primary">hans@did:key:...</code>). 
                                This is highly useful because choosing a recognizable prefix like "hans" increases your 
                                trustworthiness in the network, showing your contacts that you are willing to identify yourself.
                            </p>
                            <p className="text-sm bg-theme-subtle/30 p-4 rounded-2xl border border-theme-subtle">
                                <span className="font-bold text-theme-primary">Privacy Note:</span> In oppressive regimes or high-surveillance environments, 
                                making this additional information public alongside your DID key could be a disadvantage. 
                                Therefore, setting a personal prefix is optional. If you require anonymity, you can simply 
                                use generic terms or numbers as your prefix (e.g., <span className="font-black">"0"</span>, <span className="font-black">"pc"</span>, or <span className="font-black">"mobile"</span>).
                            </p>
                        </div>
                    </div>

                    {/* Section 2: Critical Rule */}
                    <div className="p-6 bg-rose-50 border-2 border-rose-100 rounded-3xl space-y-4">
                        <div className="flex items-center gap-3 text-rose-600">
                            <ShieldAlert size={22} />
                            <h3 className="font-black text-sm uppercase tracking-widest">The Critical Device Rule</h3>
                        </div>
                        <div className="space-y-3 text-rose-900 leading-relaxed">
                            <p className="font-bold">
                                Every single device you own must use a completely different prefix.
                            </p>
                            <p className="text-sm">
                                <span className="font-black text-rose-700 underline">Important:</span> "No prefix" (leaving the field entirely blank) also counts as a specific prefix! 
                                This means the "blank" prefix can only ever be used on <span className="font-black">one single device</span>.
                            </p>
                            <div className="bg-white/50 p-4 rounded-2xl border border-rose-200">
                                <p className="text-sm font-medium flex items-start gap-2">
                                    <AlertTriangle size={16} className="shrink-0 mt-0.5 text-rose-600" />
                                    <span>
                                        If you duplicate any prefix (including the blank one) across multiple devices, the wallet's internal 
                                        protection mechanisms (like the Double-Spend protection) will fail. 
                                        This will <span className="font-black underline">inevitably lead to severe errors</span> and 
                                        will <span className="font-black underline">irreversibly destroy your reputation</span> in the network.
                                    </span>
                                </p>
                            </div>
                        </div>
                    </div>
                </div>

                {/* Footer */}
                <div className="p-6 bg-theme-subtle/10 flex justify-center border-t border-theme-subtle">
                    <Button onClick={onClose} className="px-12 py-4 rounded-2xl shadow-premium-lg">
                        I Understand the Risks
                    </Button>
                </div>
            </div>
        </div>
    );
}
