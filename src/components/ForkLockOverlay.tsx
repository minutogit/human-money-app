import { Button } from './ui/Button';

interface ForkLockOverlayProps {
    onStartRecovery: () => void;
}

export function ForkLockOverlay({ onStartRecovery }: ForkLockOverlayProps) {
    return (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
            <div className="max-w-md w-full bg-white dark:bg-gray-800 rounded-2xl shadow-2xl overflow-hidden border border-red-500/30">
                <div className="bg-red-500 p-6 flex justify-center">
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-16 w-16 text-white animate-pulse" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                    </svg>
                </div>
                
                <div className="p-8 text-center">
                    <h2 className="text-2xl font-bold text-gray-900 dark:text-white mb-4">Security Lockdown</h2>
                    <p className="text-gray-600 dark:text-gray-300 mb-6 leading-relaxed">
                        A fork has been detected in your transaction history! This usually happens if you restore an old backup or use this wallet on multiple devices simultaneously without proper synchronization.
                    </p>
                    
                    <div className="bg-red-50 dark:bg-red-900/20 rounded-lg p-4 mb-8 text-left border border-red-100 dark:border-red-900/30">
                        <p className="text-sm text-red-700 dark:text-red-400 font-medium flex items-center gap-2">
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                                <path fillRule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z" clipRule="evenodd" />
                            </svg>
                            Action Required
                        </p>
                        <p className="text-sm text-red-600 dark:text-red-500 mt-1">
                            To ensure your identity and reputation are protected, you must perform a full wallet recovery using your seed phrase.
                        </p>
                    </div>
                    
                    <Button 
                        variant="primary" 
                        size="lg" 
                        className="w-full bg-red-600 hover:bg-red-700 text-white border-none py-4 text-lg"
                        onClick={onStartRecovery}
                    >
                        Recover Wallet with Seed Phrase
                    </Button>
                    
                    <p className="text-xs text-theme-subtle mt-4">
                        All normal wallet functions are suspended until recovery is complete.
                    </p>
                </div>
            </div>
        </div>
    );
}
