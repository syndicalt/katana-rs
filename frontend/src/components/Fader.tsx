// Vertical fader (SVG) for EQ bands: gradient fill up to value, center notch,
// value label, vertical drag with pointer capture, Shift fine, double-click reset.
import { useCallback, useRef, useState } from 'react';

export interface FaderProps {
  min: number;
  max: number;
  value: number;
  label?: string;
  display?: string;
  onChange: (v: number) => void;
  defaultValue?: number;
  width?: number;
  height?: number;
  disabled?: boolean;
}

export default function Fader({
  min,
  max,
  value,
  label,
  display,
  onChange,
  defaultValue,
  width = 44,
  height = 160,
  disabled = false,
}: FaderProps) {
  const [dragging, setDragging] = useState(false);
  const dragRef = useRef<{ startY: number; startValue: number } | null>(null);

  const clampVal = (v: number) => Math.min(max, Math.max(min, v));
  const norm = (v: number) => (max <= min ? 0 : Math.min(1, Math.max(0, (v - min) / (max - min))));
  const t = norm(value);
  const y = 10 + (1 - t) * (height - 20);
  const notchY = 10 + (1 - norm(0)) * (height - 20);

  const handlePointer = useCallback(
    (e: React.PointerEvent) => {
      if (disabled || !dragRef.current) return;
      const dy = dragRef.current.startY - e.clientY;
      const span = max - min;
      const step = span > 0 ? span / (e.shiftKey ? 600 : 160) : 0;
      onChange(clampVal(dragRef.current.startValue + dy * step));
    },
    [disabled, max, min, onChange],
  );

  const onPointerDown = (e: React.PointerEvent) => {
    if (disabled) return;
    e.preventDefault();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    dragRef.current = { startY: e.clientY, startValue: value };
    setDragging(true);
  };
  const onPointerUp = (e: React.PointerEvent) => {
    dragRef.current = null;
    setDragging(false);
    try {
      (e.currentTarget as Element).releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
  };

  return (
    <div className={`flex flex-col items-center gap-1 select-none ${disabled ? 'opacity-50' : ''}`}>
      {label && <div className="text-[10px] text-amp-400">{label}</div>}
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        className={`drag-none ${dragging ? '' : 'knob-idle'}`}
        onPointerDown={onPointerDown}
        onPointerMove={dragging ? handlePointer : undefined}
        onPointerUp={onPointerUp}
        onDoubleClick={(e) => {
          e.stopPropagation();
          if (defaultValue !== undefined) onChange(defaultValue);
        }}
        role="slider"
        aria-label={label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
      >
        <defs>
          <linearGradient id={`fadergrad-${label ?? 'x'}`} x1="0%" y1="100%" x2="0%" y2="0%">
            <stop offset="0%" stopColor="#3f7fb0" />
            <stop offset="100%" stopColor="#5ca6e0" />
          </linearGradient>
        </defs>
        <rect x={width / 2 - 3} y={10} width={6} height={height - 20} rx={3} fill="#2c2c38" />
        <rect x={width / 2 - 3} y={y} width={6} height={height - 20 - (y - 10)} rx={3} fill={`url(#fadergrad-${label ?? 'x'})`} />
        <line x1={width / 2 - 7} y1={notchY} x2={width / 2 + 7} y2={notchY} stroke="#55555f" strokeWidth={1} />
        <rect x={width / 2 - 8} y={y - 5} width={16} height={10} rx={3} fill="#e8e8f0" />
      </svg>
      {display !== undefined && <div className="text-[10px] font-mono text-amp-300">{display}</div>}
    </div>
  );
}
