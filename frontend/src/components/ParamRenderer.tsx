// Renders one schema parameter with the strict control rules:
//   boolean (2 options)  -> Toggle
//   exactly 3 options    -> Knob (notched, discrete)
//   4..32 options        -> SelectorKnob (turn + search popup)
//   >32 options / range  -> arc Knob (continuous)
//   EQ band              -> Fader (pass fader to force)
//   data                 -> read-only hex display
import type { Param } from '../lib/api';
import LABELS from '../lib/paramLabels.json';
import { useStore } from '../lib/store';
import { clamp, mapRawToValue, mapValueToRaw } from '../lib/map';
import Knob from './Knob';
import Toggle from './Toggle';
import SelectorKnob from './SelectorKnob';
import Fader from './Fader';

const labelFor = (p: Param | undefined, fallback: string): string => {
  if (!p) return fallback;
  const key = `0x${p.section.toString(16).padStart(2, '0').toUpperCase()}:0x${p.index.toString(16).padStart(2, '0').toUpperCase()}`;
  const custom = (LABELS as Record<string, string>)[key];
  if (custom) return custom;
  const cleaned = p.label.replace(/:$/, '').trim();
  return cleaned || fallback;
};

export function hexIndex(i: number): string {
  return i.toString(16).toUpperCase().padStart(2, '0');
}

/** Map an enum param to its option list ({raw,label}); [] if not enum. */
function enumOpts(p: Param | undefined): { raw: number; label: string }[] {
  return p?.enum_values ?? [];
}

function Readonly({ p }: { p: Param }) {
  return (
    <div className="flex flex-col items-center gap-1 text-center">
      <span className="font-mono text-xs text-amp-200">{p.display || p.raw.toString(16).toUpperCase().padStart(2, '0')}</span>
      <span className="text-[10px] uppercase tracking-wide text-amp-400">{hexIndex(p.index)}</span>
    </div>
  );
}

export interface RenderOptions {
  section: number;
  index: number;
  /** Explicit label; defaults to the curated schema label map. */
  label?: string;
  /** Force Fader (EQ bands). */
  fader?: boolean;
}

/** Render any param following the control decision rules. */
export default function ParamRenderer({ section, index, label, fader = false }: RenderOptions) {
  const { state, setParam } = useStore();
  const p = state.params.find((q) => q.section === section && q.index === index);
  if (!p) return null;
  const labelFinal = label ?? labelFor(p, hexIndex(index));

  if (p.kind === 'data') return <Readonly p={p} />;

  if (fader) {
    if (p.kind !== 'range' || !p.range) return null;
    const r = p.range;
    const v = mapRawToValue(p.raw, r);
    const decimals = Math.abs(r.data_max - r.data_min) < 1 ? 1 : 0;
    return (
      <Fader
        min={r.data_min}
        max={r.data_max}
        value={v}
        label={labelFinal}
        display={v.toFixed(decimals)}
        onChange={(nv) => setParam(section, index, mapValueToRaw(nv, r))}
        defaultValue={0}
        height={140}
      />
    );
  }

  if (p.kind === 'range') {
    if (!p.range) return null;
    const r = p.range;
    const v = mapRawToValue(p.raw, r);
    const decimals = Math.abs(r.data_max - r.data_min) < 1 ? 1 : 0;
    const mid = clamp((r.raw_min + r.raw_max) / 2, r.raw_min, r.raw_max);
    return (
      <Knob
        value={v}
        min={r.data_min}
        max={r.data_max}
        label={labelFinal}
        display={`${v.toFixed(decimals)}${r.suffix}`}
        onChange={(nv) => setParam(section, index, mapValueToRaw(nv, r))}
        defaultValue={mapRawToValue(Math.round(mid), r)}
      />
    );
  }

  const opts = enumOpts(p);
  if (opts.length === 2) {
    const onRaw = opts[1].raw;
    return (
      <Toggle
        on={p.raw === onRaw}
        label={labelFinal}
        onChange={(on) => setParam(section, index, on ? onRaw : opts[0].raw)}
      />
    );
  }
  if (opts.length === 3) {
    return (
      <Knob
        value={p.raw}
        min={0}
        max={2}
        label={labelFinal}
        display={p.display}
        onChange={(raw) => setParam(section, index, raw)}
        mode="discrete"
        options={opts}
        defaultValue={opts[0].raw}
      />
    );
  }
  if (opts.length <= 32 && opts.length > 0) {
    return (
      <SelectorKnob
        value={p.raw}
        options={opts}
        label={labelFinal}
        onChange={(raw) => setParam(section, index, raw)}
      />
    );
  }
  const max = Math.max(...opts.map((o) => o.raw), 1);
  return (
    <Knob
      value={p.raw}
      min={0}
      max={max}
      label={labelFinal}
      display={p.display}
      onChange={(raw) => setParam(section, index, Math.round(clamp(raw, 0, max)))}
      defaultValue={Math.round(max / 2)}
    />
  );
}
