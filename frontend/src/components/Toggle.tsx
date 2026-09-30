// Pill toggle switch with glow when on.
export interface ToggleProps {
  on: boolean;
  label?: string;
  onChange: (on: boolean) => void;
  disabled?: boolean;
}

export default function Toggle({ on, label, onChange, disabled = false }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      disabled={disabled}
      onClick={() => onChange(!on)}
      className={`flex items-center gap-2 rounded-full border px-1 py-1 transition-colors ${
        disabled ? 'opacity-50' : ''
      } ${on ? 'border-accent-dim bg-amp-800' : 'border-[#2c2c38] bg-amp-900'}`}
    >
      <span
        className={`h-4 w-7 rounded-full p-0.5 transition-all ${on ? 'bg-accent' : 'bg-[#2c2c38]'}`}
        style={on ? { boxShadow: '0 0 8px rgba(92,166,224,0.7)' } : undefined}
      >
        <span
          className={`block h-3 w-3 rounded-full bg-amp-950 transition-transform ${on ? 'translate-x-3' : ''}`}
        />
      </span>
      {label && <span className="pr-2 text-xs text-amp-200">{label}</span>}
    </button>
  );
}
