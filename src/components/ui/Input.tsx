// src/components/ui/Input.tsx
import React from 'react';

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {}

export const Input = React.forwardRef<HTMLInputElement, InputProps>(
    ({ className, type, ...props }, ref) => {
        const baseStyle =
            "w-full p-3 border border-gray-300 rounded-lg text-base text-gray-900 " +
            "transition-shadow duration-150 ease-in-out " +
            "focus:outline-none focus:ring-2 focus:ring-primary focus:border-transparent " + // Deutlicherer Fokus
            "placeholder:text-gray-400"; // Subtilerer Platzhalter

        return (
            <input
                type={type}
                className={`${baseStyle} ${className}`}
                ref={ref}
                {...props}
            />
        );
    }
);