// Circular SVG rotary knob with vertical-drag editing, fine adjust via Shift,
// double-click reset, and touch support (pointer capture).
// Modes:
//  - continuous: any numeric value between min..max (default)
//  - discrete:   exactly N options; drag snaps to option index; tick marks
//  - selector:   like discrete (turn through options) but click opens an
//                external search popup (onOpen)
import { useCallback, useEffect, useRef, useState } from 'react';

export interface KnobOption {
  raw: number;
  label: string;
}

export interface KnobProps {
  /** Normalized value 0..1, or raw/display value between min/max. */
  value: number;
  min: number;
  max: number;
  /** Label shown under the knob. */
  label?: string;
  /** Value text shown under the center (defaults to nothing). */
  display?: string;
  /** Called with the new value (same space as value/min/max). */
  onChange: (v: number) => void;
  /** Value restored on double-click. */
  defaultValue?: number;
  size?: number;
  disabled?: boolean;
  /** control behavior: continuous (default), discrete (snap to options), selector (turn + click opens popup). */
  mode?: 'continuous' | 'discrete' | 'selector';
  /** Options for discrete/selector modes (ordered; raw must be unique). */
  options?: KnobOption[];
  /** Called when a selector knob is clicked (without dragging). */
  onOpen?: () => void;
}

const START = -135; // degrees, arc start (bottom-left)
const SWEEP = 270; // degrees of travel

function polar(cx: number, cy: number, r: number, deg: number): [number, number] {
  const rad = ((deg - 90) * Math.PI) / 180;
  return [cx + r * Math.cos(rad), cy + r * Math.sin(rad)];
}

function describeArc(cx: number, cy: number, r: number, a0: number, a1: number): string {
  if (a1 - a0 <= 0.01) a1 = a0 + 0.01;
  const [sx, sy] = polar(cx, cy, r, a0);
  const [ex, ey] = polar(cx, cy, r, a1);
  const large = a1 - a0 > 180 ? 1 : 0;
  return `M ${sx} ${sy} A ${r} ${r} 0 ${large} 1 ${ex} ${ey}`;
}

export default function Knob({
  value,
  min,
  max,
  label,
  display,
  onChange,
  defaultValue,
  size = 64,
  disabled = false,
  mode = 'continuous',
  options,
  onOpen,
}: KnobProps) {
  const [dragging, setDragging] = useState(false);
  const dragRef = useRef<{ startY: number; startValue: number; moved: boolean } | null>(null);
  const sizeRef = useRef(size);
  sizeRef.current = size;

  // For discrete/selector modes the value is a raw byte; position is by option
  // index so irregular raws (e.g. amp types 0x00,0x08,0x0B...) are evenly spaced.
  const stepped = mode !== 'continuous' && options && options.length > 1;
  const optIndex = stepped
    ? Math.max(0, options!.findIndex((o) => o.raw === value))
    : 0;
  const n = stepped ? options!.length : 0;

  const norm = stepped
    ? (n <= 1 ? 0 : optIndex / (n - 1))
    : max <= min
      ? 0
      : Math.min(1, Math.max(0, (value - min) / (max - min)));
  const angle = START + norm * SWEEP;
  const cx = size / 2;
  const cy = size / 2;
  const r = size / 2 - 9;
  const [sx, sy] = polar(cx, cy, r, angle);

  const handlePointer = useCallback(
    (e: React.PointerEvent) => {
      if (disabled || !dragRef.current) return;
      const fine = e.shiftKey;
      const dy = dragRef.current.startY - e.clientY;
      if (Math.abs(dy) > 3) dragRef.current.moved = true;
      if (stepped) {
        const span = n - 1;
        if (span <= 0) return;
        const step = span / (fine ? 800 : 200);
        const idx = Math.round(Math.min(span, Math.max(0, optIndex + dy * step)));
        const next = options![idx].raw;
        if (next !== value) onChange(next);
      } else {
        const span = max - min;
        const step = span > 0 ? span / (fine ? 600 : 160) : 0;
        const nv = dragRef.current.startValue + dy * step;
        onChange(Math.min(max, Math.max(min, nv)));
      }
    },
    [disabled, max, min, onChange, stepped, n, optIndex, options, value],
  );

  const onPointerDown = (e: React.PointerEvent) => {
    if (disabled) return;
    e.preventDefault();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    dragRef.current = { startY: e.clientY, startValue: value, moved: false };
    setDragging(true);
  };
  const onPointerMove = (e: React.PointerEvent) => {
    if (dragging) handlePointer(e);
  };
  const onPointerUp = (e: React.PointerEvent) => {
    const d = dragRef.current;
    dragRef.current = null;
    setDragging(false);
    try {
      (e.currentTarget as Element).releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
    if (d && !d.moved && mode === 'selector' && onOpen) onOpen();
  };

  // Keep drag baseline stable if the value changes while dragging (live events).
  useEffect(() => {
    if (dragging && dragRef.current) dragRef.current.startValue = value;
  }, [value, dragging]);

  const shown = stepped && options ? options[optIndex]?.label ?? display : display;

  return (
    <div
      className={`flex flex-col items-center gap-1 select-none ${disabled ? 'opacity-50' : ''}`}
      style={{ width: size + 8 }}
      title={stepped && options ? options.map((o) => o.label).join(' · ') : undefined}
    >
      <svg
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        className={`drag-none ${dragging ? '' : 'knob-idle'}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onDoubleClick={(e) => {
          e.stopPropagation();
          if (defaultValue !== undefined) onChange(defaultValue);
        }}
        role="slider"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={Math.max(1, n - 1)}
        aria-valuenow={stepped ? optIndex : value}
      >
        <defs>
          <linearGradient id={`knobgrad-${label ?? 'x'}`} x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#5ca6e0" />
            <stop offset="100%" stopColor="#3f7fb0" />
          </linearGradient>
        </defs>
        {/* track */}
        <path d={describeArc(cx, cy, r, START, START + SWEEP)} stroke="#2c2c38" strokeWidth={4} fill="none" strokeLinecap="round" />
        {/* active arc */}
        <path
          d={describeArc(cx, cy, r, START, angle)}
          stroke={`url(#knobgrad-${label ?? 'x'})`}
          strokeWidth={4}
          fill="none"
          strokeLinecap="round"
        />
        {/* step tick marks for discrete/selector */}
        {stepped &&
          options!.map((o, i) => {
            const a = START + (n <= 1 ? 0 : (i / (n - 1)) * SWEEP);
            const [tx, ty] = polar(cx, cy, r - 7, a);
            return <circle key={o.raw} cx={tx} cy={ty} r={1.2} fill={i === optIndex ? '#e8e8f0' : '#5c5c6a'} />;
          })}
        {/* disc */}
        <circle cx={cx} cy={cy} r={r - 8} fill="#171922" stroke="#2c2c38" strokeWidth={1} />
        <circle cx={cx} cy={cy} r={r - 13} fill="#0f1016" />
        {/* indicator */}
        <line x1={cx} y1={cy} x2={sx} y2={sy} stroke="#e8e8f0" strokeWidth={2} strokeLinecap="round" />
        <circle cx={cx} cy={cy} r={3} fill="#d8d8e2" />
      </svg>
      {shown !== undefined && <div className="text-[10px] leading-none text-amp-300 font-mono">{shown}</div>}
      {label && <div className="text-[10px] leading-none text-amp-400 uppercase tracking-wide text-center">{label}</div>}
    </div>
  );
}
