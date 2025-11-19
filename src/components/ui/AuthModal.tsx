import { useState, useEffect } from 'react';
import { Button } from './Button';
import { Input } from './Input';

interface AuthModalProps {
    isOpen: boolean;
    onConfirm: (password: string) => void;
    onCancel: () => void;
    title?: string;
    description?: string;
}

export function AuthModal({ isOpen, onConfirm, onCancel, title, description }: AuthModalProps) {
    const [password, setPassword] = useState('');

    // Reset password field when modal opens
    useEffect(() => {
        if (isOpen) setPassword('');
    }, [isOpen]);

    if (!isOpen) return null;

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        onConfirm(password);
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
            <div className="w-full max-w-md rounded-lg bg-bg-card border border-theme-subtle p-6 shadow-2xl animate-in fade-in zoom-in-95 duration-200">
                <h2 className="text-xl font-bold text-theme-primary mb-2">{title || "Authentication Required"}</h2>
                <p className="text-sm text-theme-light mb-6">{description || "Please enter your wallet password to continue."}</p>
                
                <form onSubmit={handleSubmit} className="space-y-4">
                    <Input 
                        type="password" 
                        value={password} 
                        onChange={(e) => setPassword(e.target.value)}
                        placeholder="Wallet Password"
                        autoFocus
                        required
                    />
                    <div className="flex justify-end gap-3 mt-6">
                        <Button type="button" variant="secondary" onClick={onCancel}>Cancel</Button>
                        <Button type="submit" disabled={!password}>Confirm</Button>
                    </div>
                </form>
            </div>
        </div>
    );
}