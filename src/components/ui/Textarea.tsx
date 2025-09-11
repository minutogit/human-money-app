// src/components/ui/Textarea.tsx
import React from 'react';

interface TextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {}

export const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
    ({ className, ...props }, ref) => {
        const baseStyle =
            "w-full p-3 border border-gray-300 rounded-lg font-mono text-base text-gray-900 resize-none " +
            "transition-shadow duration-150 ease-in-out " +
            "focus:outline-none focus:ring-2 focus:ring-primary focus:border-transparent " + // Deutlicherer Fokus
            "placeholder:text-gray-400";

        return (
            <textarea
                className={`${baseStyle} ${className}`}
                ref={ref}
                {...props}
            />
        );
    }
);