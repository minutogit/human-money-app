import { Button } from './Button';

interface ConfirmationModalProps {
    isOpen: boolean;
    title: string;
    description: React.ReactNode; // ReactNode erlaubt auch HTML/JSX im Text
    confirmText?: string;
    cancelText?: string;
    onConfirm: () => void;
    onCancel: () => void;
    isProcessing?: boolean;
}

export function ConfirmationModal({ 
    isOpen, 
    title, 
    description, 
    confirmText = "Confirm", 
    cancelText = "Cancel", 
    onConfirm, 
    onCancel,
    isProcessing = false
}: ConfirmationModalProps) {
    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
            <div className="w-full max-w-md rounded-lg bg-bg-card border border-theme-subtle p-6 shadow-2xl animate-in fade-in zoom-in-95 duration-200">
                <h2 className="text-xl font-bold text-theme-primary mb-2">{title}</h2>
                <div className="text-sm text-theme-light mb-6">{description}</div>
                <div className="flex justify-end gap-3">
                    <Button variant="secondary" onClick={onCancel} disabled={isProcessing}>
                        {cancelText}
                    </Button>
                    <Button onClick={onConfirm} disabled={isProcessing}>
                        {isProcessing ? "Processing..." : confirmText}
                    </Button>
                </div>
            </div>
        </div>
    );
}