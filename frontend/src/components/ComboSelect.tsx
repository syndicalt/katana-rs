// Dark-styled native select (reliable on Tauri webviews).
export interface ComboSelectProps {
  value: number;
  options: { raw: number; label: string }[];
  onChange: (raw: number) => void;
  label?: string;
  disabled?: boolean;
}

export default function ComboSelect({ value, options, onChange, label, disabled = false }: ComboSelectProps) {
  const has = options.some((o) => o.raw === value);
  return (
    <label className={`flex flex-col gap-1 ${disabled ? 'opacity-50' : ''}`}>
      {label && <span className="text-[10px] uppercase tracking-wide text-amp-400">{label}</span>}
      <select
        value={has ? value : -1}
        disabled={disabled}
        onChange={(e) => onChange(Number(e.target.value))}
        className="rounded-md border border-[#2c2c38] bg-amp-800 px-2 py-1.5 text-xs text-amp-100 outline-none focus:border-accent"
      >
        {!has && <option value={-1}>— {value.toString(16).toUpperCase().padStart(2, '0')}</option>}
        {options.map((o) => (
          <option key={o.raw} value={o.raw}>
            {o.label || o.raw.toString(16).toUpperCase()}
          </option>
        ))}
      </select>
    </label>
  );
}
