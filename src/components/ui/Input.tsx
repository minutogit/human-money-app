// src/components/ui/Input.tsx
import React from 'react';

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {}

export const Input = React.forwardRef<HTMLInputElement, InputProps>(
    ({ className, type, ...props }, ref) => {
        return (
            <input
                type={type}
                className={`w-full p-3 border rounded-lg text-base border-theme-light-border text-theme-secondary transition-shadow duration-150 ease-in-out focus:outline-none focus:ring-2 focus:ring-theme-primary focus:border-transparent placeholder-theme-placeholder ${className}`}
                ref={ref}
                {...props}
            />
        );
    }
);