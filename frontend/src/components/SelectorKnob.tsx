// Selector knob: turns through options (discrete) and exposes a searchable
// popup list (Neural-DSP style) opened by clicking the knob. Used for Amp
// Type, FX Type, Output/Cab selects — anything with 4..32 options.
import { useEffect, useRef, useState } from 'react';
import Knob, { type KnobOption } from './Knob';

export interface SelectorKnobProps {
  value: number;
  options: KnobOption[];
  label?: string;
  onChange: (raw: number) => void;
  disabled?: boolean;
  size?: number;
  /** Optional hint displayed above the popup list. */
  hint?: string;
}

// Constant to avoid re-creating; popup renders in a fixed overlay layer.
function Popup({
  open,
  anchor,
  options,
  value,
  onChange,
  onClose,
  hint,
}: {
  open: boolean;
  anchor: HTMLElement | null;
  options: KnobOption[];
  value: number;
  onChange: (raw: number) => void;
  onClose: () => void;
  hint?: string;
}) {
  const [q, setQ] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const selectedRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (open) {
      setQ('');
      // focus + scroll to current selection after mount
      requestAnimationFrame(() => {
        inputRef.current?.focus();
        selectedRef.current?.scrollIntoView({ block: 'center' });
      });
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open || !anchor) return null;
  const rect = anchor.getBoundingClientRect();
  const ql = q.trim().toLowerCase();
  const filtered = ql
    ? options.filter((o) => o.label.toLowerCase().includes(ql))
    : options;

  return (
    <div
      className="sk-popup fixed z-50 w-72 max-h-[60vh] flex flex-col rounded-xl border border-[#3b4d65] bg-[#161922] shadow-xl shadow-black/60"
      style={{ left: Math.min(rect.left, window.innerWidth - 300), top: Math.min(rect.bottom + 6, window.innerHeight - 40) }}
      onPointerDown={(e) => e.stopPropagation()}
    >
      {hint && <div className="px-3 pt-2 text-[10px] uppercase tracking-wide text-amp-400">{hint}</div>}
      <input
        ref={inputRef}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder="Search…"
        className="mx-2 my-2 rounded-md border border-[#2c2c38] bg-[#0f1016] px-2 py-1.5 text-xs text-amp-100 outline-none placeholder:text-amp-500 focus:border-accent"
      />
      <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {filtered.length === 0 && <p className="px-2 py-3 text-xs text-amp-400">No matches</p>}
        {filtered.map((o) => {
          const active = o.raw === value;
          return (
            <button
              key={o.raw}
              ref={active ? selectedRef : undefined}
              type="button"
              onClick={() => {
                onChange(o.raw);
                onClose();
              }}
              className={`block w-full rounded-md px-2 py-1.5 text-left text-xs transition-colors ${
                active ? 'bg-accent text-amp-950' : 'text-amp-200 hover:bg-[#1d2130]'
              }`}
            >
              {o.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}

export default function SelectorKnob({
  value,
  options,
  label,
  onChange,
  disabled = false,
  size = 64,
  hint,
}: SelectorKnobProps) {
  const [open, setOpen] = useState(false);
  const anchorRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!anchorRef.current?.contains(e.target as Node)) {
        const popup = document.querySelector('.sk-popup');
        if (!popup?.contains(e.target as Node)) setOpen(false);
      }
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open]);

  return (
    <span ref={anchorRef} className="relative inline-flex">
      <Knob
        value={value}
        min={0}
        max={Math.max(1, options.length - 1)}
        label={label}
        display={options.find((o) => o.raw === value)?.label}
        onChange={onChange}
        mode="selector"
        options={options}
        onOpen={() => setOpen((o) => !o)}
        disabled={disabled}
        size={size}
      />
      <Popup
        open={open}
        anchor={open ? anchorRef.current : null}
        options={options}
        value={value}
        onChange={onChange}
        onClose={() => setOpen(false)}
        hint={hint}
      />
    </span>
  );
}
