// src/components/ui/Button.tsx
import React from 'react';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
    ({ className, children, ...props }, ref) => {
        return (
            <button
                className={`w-auto min-w-[200px] px-6 py-4 rounded-lg font-bold text-xl border-2 border-transparent transition-all duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-theme-primary shadow-md hover:shadow-lg bg-theme-primary hover:bg-theme-accent text-white disabled:cursor-not-allowed disabled:opacity-75 mx-auto block ${className}`}
                ref={ref}
                {...props}
            >
                {children}
            </button>
        );
    }
);