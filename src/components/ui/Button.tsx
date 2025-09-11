// src/components/ui/Button.tsx
import React from 'react';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
    ({ className, children, ...props }, ref) => {
        const baseStyle =
            "w-full p-3 rounded-lg font-semibold text-lg border-2 border-transparent " +
            "transition-all duration-200 ease-in-out " + // Sanftere Übergänge
            "focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-background"; // Besserer Fokus-Effekt

        const primaryStyle =
            "bg-primary text-white hover:bg-primary-hover " +
            "focus:ring-primary disabled:bg-gray-400 disabled:cursor-not-allowed";

        return (
            <button
                className={`${baseStyle} ${primaryStyle} ${className}`}
                ref={ref}
                {...props}
            >
                {children}
            </button>
        );
    }
);