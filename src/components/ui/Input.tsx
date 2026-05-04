// src/components/ui/Input.tsx
import React from 'react';

type InputProps = React.InputHTMLAttributes<HTMLInputElement>;

export const Input = React.forwardRef<HTMLInputElement, InputProps>(
    ({ className, type, ...props }, ref) => {
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
