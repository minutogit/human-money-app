import { Button } from './Button';
import { HelpCircle, ShieldAlert } from 'lucide-react';

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
                        <h2 className="text-2xl font-black text-theme-primary tracking-tight">Device Prefixes & Sub-Accounts</h2>
                    </div>
                    <p className="text-theme-light font-medium">Learn how to configure your multi-device sub-accounts safely.</p>
                </div>

                {/* Content */}
                <div className="p-8 space-y-6 max-h-[60vh] overflow-y-auto custom-scrollbar">
                    {/* Q1 */}
                    <div className="space-y-2">
                        <h3 className="font-bold text-base text-theme-primary">❓ What is a Device Prefix / Sub-Account?</h3>
                        <p className="text-sm text-theme-secondary leading-relaxed">
                            When you use the same seed phrase on multiple devices, they all share the same base cryptographic key (<code className="bg-theme-primary/5 px-1.5 py-0.5 rounded font-bold text-theme-primary">did:key:...</code>).
                            To prevent the devices from clashing (e.g. using the same transaction sequence numbers), each device needs a unique prefix (like <code className="bg-theme-primary/5 px-1.5 py-0.5 rounded font-bold text-theme-primary">laptop</code> or <code className="bg-theme-primary/5 px-1.5 py-0.5 rounded font-bold text-theme-primary">phone</code>). This splits the wallet into separate, safe branches.
                        </p>
                    </div>

                    {/* Q2 */}
                    <div className="space-y-2">
                        <h3 className="font-bold text-base text-theme-primary">🏢 Can I use it for branding / recognition?</h3>
                        <p className="text-sm text-theme-secondary leading-relaxed">
                            Yes! The prefix is visible to your contacts (e.g., <code className="bg-theme-primary/5 px-1.5 py-0.5 rounded font-bold text-theme-primary">my-company:abc@did:key:...</code>).
                            Using a recognizable name (like your name or business name) makes it extremely easy for your partners to quickly identify and verify payments from you.
                        </p>
                    </div>

                    {/* Q3 */}
                    <div className="space-y-2">
                        <h3 className="font-bold text-base text-theme-primary">⚙️ Is it optional? What if I leave it blank?</h3>
                        <p className="text-sm text-theme-secondary leading-relaxed">
                            <strong>Yes, it is optional.</strong> If you only use this wallet on this single device, leave the field empty. This creates a standard "Root Account". You do not lose any features or security.
                        </p>
                    </div>

                    {/* Q4 */}
                    <div className="space-y-2">
                        <h3 className="font-bold text-base text-theme-primary">✏️ Can I change or set the prefix later?</h3>
                        <p className="text-sm text-theme-secondary leading-relaxed">
                            A prefix is permanently tied to a profile once it is created. However, you can always <strong>create another profile</strong> (sub-account) on the same device using the same seed phrase but with a different prefix. <strong>You do not need to delete your old profile to do this.</strong>
                        </p>
                    </div>

                    {/* Safety Alert */}
                    <div className="p-5 bg-rose-50 border border-rose-100 rounded-2xl space-y-3">
                        <div className="flex items-center gap-2 text-rose-600">
                            <ShieldAlert size={18} />
                            <h4 className="font-black text-xs uppercase tracking-wider">Critical Safety Rule</h4>
                        </div>
                        <p className="text-xs text-rose-900 leading-relaxed font-medium">
                            Every active device using the same seed phrase <strong>must use a different prefix</strong>.
                        </p>
                        <p className="text-xs text-rose-800 leading-relaxed">
                            Using the exact same prefix (including leaving it blank) on two devices at the same time will cause sequence collisions. This will result in severe errors and inevitably damage your reputation in the network.
                        </p>
                    </div>
                </div>

                {/* Footer */}
                <div className="p-6 bg-theme-subtle/10 flex justify-center border-t border-theme-subtle">
                    <Button onClick={onClose} className="px-12 py-4 rounded-2xl shadow-premium-lg">
                        I Understand
                    </Button>
                </div>
            </div>
        </div>
    );
}
