// src/components/ui/Textarea.tsx
import React from 'react';

interface TextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {}

export const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
    ({ className, ...props }, ref) => {
        return (
            <textarea
                className={`w-full p-3 border rounded-lg font-mono text-base resize-none border-theme-light-border text-theme-secondary transition-shadow duration-150 ease-in-out focus:outline-none focus:ring-2 focus:ring-theme-primary focus:border-transparent placeholder-theme-placeholder ${className}`}
                ref={ref}
                {...props}
            />
        );
    }
);