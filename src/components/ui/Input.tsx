// src/components/ui/Input.tsx
import React, { useState } from 'react';
import { Eye, EyeOff } from 'lucide-react';

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
    showToggle?: boolean;
}

export const Input = React.forwardRef<HTMLInputElement, InputProps>(
    ({ className = '', type, showToggle = false, ...props }, ref) => {
        const [revealed, setRevealed] = useState(false);

        if (type === 'password' && showToggle) {
            return (
                <div className="relative w-full">
                    <input
                        type={revealed ? 'text' : 'password'}
                        className={`w-full px-4 py-3 pr-10 bg-white border border-theme-subtle rounded-xl text-theme-secondary shadow-inner-soft transition-all duration-300 focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:border-theme-primary focus:bg-white placeholder-theme-placeholder/50 ${className}`}
                        ref={ref}
                        {...props}
                    />
                    <button
                        type="button"
                        tabIndex={-1}
                        onClick={() => setRevealed((prev) => !prev)}
                        data-testid="input-visibility-toggle"
                        aria-label={revealed ? 'Hide' : 'Show'}
                        className="absolute right-2.5 top-1/2 -translate-y-1/2 p-1 text-theme-light hover:text-theme-secondary transition-colors cursor-pointer"
                    >
                        {revealed ? <EyeOff size={16} /> : <Eye size={16} />}
                    </button>
                </div>
            );
        }

        return (
            <input
                type={type}
                className={`w-full px-4 py-3 bg-white border border-theme-subtle rounded-xl text-theme-secondary shadow-inner-soft transition-all duration-300 focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:border-theme-primary focus:bg-white placeholder-theme-placeholder/50 ${className}`}
                ref={ref}
                {...props}
            />
        );
    }
);
Input.displayName = 'Input';
