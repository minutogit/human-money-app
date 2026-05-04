import React from "react";

interface AuthLayoutProps {
  children: React.ReactNode;
  maxWidth?: string;
  className?: string;
}

/**
 * Shared layout for all unauthenticated screens (Login, Recovery, Profile Creation).
 * Provides a consistent background, responsive padding, and premium card styling.
 */
export function AuthLayout({ 
  children, 
  maxWidth = "max-w-2xl", 
  className = "" 
}: AuthLayoutProps) {
  return (
    <div className="w-full min-h-screen flex items-start sm:items-center justify-center py-4 sm:py-6 px-4 relative overflow-hidden">
      {/* Premium Background Elements */}
      <div className="absolute top-0 left-1/4 w-96 h-96 bg-theme-primary/5 rounded-full blur-3xl -translate-y-1/2"></div>
      <div className="absolute bottom-0 right-1/4 w-96 h-96 bg-theme-primary/5 rounded-full blur-3xl translate-y-1/2"></div>
      
      {/* Card Container */}
      <div className={`w-full ${maxWidth} bg-white/80 backdrop-blur-xl border border-theme-subtle rounded-[48px] p-6 sm:p-10 sm:pb-12 shadow-premium-lg space-y-6 sm:space-y-8 relative animate-in fade-in zoom-in duration-700 ${className}`}>
        {/* Decorative Top Line */}
        <div className="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-transparent via-theme-primary/20 to-transparent"></div>
        
        {children}
      </div>
    </div>
  );
}
