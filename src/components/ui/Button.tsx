// src/components/ui/Button.tsx
import React from 'react';

// Define the variants and their corresponding styles
const variants = {
    primary: 'bg-theme-primary text-white border-transparent hover:bg-theme-accent focus:ring-theme-primary',
    secondary: 'bg-theme-subtle text-theme-secondary border-theme-subtle/50 hover:bg-bg-app hover:border-theme-light focus:ring-theme-primary',
    outline: 'bg-transparent text-theme-primary border-current hover:bg-theme-primary hover:text-white focus:ring-theme-primary',
    danger: 'bg-theme-error text-white border-transparent hover:opacity-90 focus:ring-theme-error',
};

// Define the sizes and their corresponding styles
const sizes = {
    lg: 'px-6 py-3 text-lg font-bold rounded-lg',
    md: 'px-4 py-2 text-base font-semibold rounded-md',
    sm: 'px-3 py-1.5 text-sm font-medium rounded-md',
    xs: 'px-2 py-1 text-[11px] font-bold rounded',
};

// Define the component's props, including the new variant and size props
export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
    variant?: keyof typeof variants;
    size?: keyof typeof sizes;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
    ({ className, children, variant = 'primary', size = 'md', ...props }, ref) => {

        // Combine base styles with variant, size, and any additional classes
        const finalClassName = [
            'inline-flex items-center justify-center',
            'border',
            'transition-all duration-200 ease-in-out',
            'focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-bg-app',
            'disabled:cursor-not-allowed disabled:opacity-60',
            'shadow-sm hover:shadow-md',
            variants[variant],
            sizes[size],
            className,
        ].filter(Boolean).join(' ');

        return (
            <button
                className={finalClassName}
                ref={ref}
                {...props}
            >
                {children}
            </button>
        );
    }
);