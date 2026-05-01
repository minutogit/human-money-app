// src/components/ui/Button.tsx
import React from 'react';

// Define the variants and their corresponding styles
const variants = {
    primary: 'bg-gradient-to-r from-theme-primary to-theme-accent text-white border-transparent hover:brightness-110 focus:ring-theme-primary shadow-md hover:shadow-lg',
    secondary: 'bg-white text-theme-secondary border-theme-subtle hover:bg-bg-app hover:border-theme-light focus:ring-theme-primary',
    outline: 'bg-transparent text-theme-primary border-current hover:bg-theme-primary hover:text-white focus:ring-theme-primary',
    danger: 'bg-theme-error text-white border-transparent hover:opacity-90 focus:ring-theme-error',
};

// Define the sizes and their corresponding styles
const sizes = {
    lg: 'px-8 py-4 text-lg font-bold rounded-2xl',
    md: 'px-6 py-2.5 text-base font-semibold rounded-xl',
    sm: 'px-4 py-2 text-sm font-medium rounded-lg',
    xs: 'px-3 py-1.5 text-[11px] font-bold rounded-md',
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
            'transition-all duration-300 ease-out',
            'focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-bg-app',
            'disabled:cursor-not-allowed disabled:opacity-60',
            'active:scale-[0.98]',
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