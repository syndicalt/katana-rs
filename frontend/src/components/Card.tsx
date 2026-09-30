// Shared card container used by all views.
import type { ReactNode } from 'react';

export interface CardProps {
  title: string;
  subtitle?: string;
  children: ReactNode;
  className?: string;
}

export default function Card({ title, subtitle, children, className = '' }: CardProps) {
  return (
    <section
      className={`rounded-xl border border-[#2c2c38] bg-amp-900 p-4 shadow-md shadow-black/30 ${className}`}
    >
      <header className="mb-3 flex items-baseline justify-between gap-2">
        <h2 className="text-[13px] font-semibold uppercase tracking-wider text-accent">{title}</h2>
        {subtitle && <span className="text-[10px] text-amp-400">{subtitle}</span>}
      </header>
      {children}
    </section>
  );
}
