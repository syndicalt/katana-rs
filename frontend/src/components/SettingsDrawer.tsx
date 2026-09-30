// Settings drawer: Output panel (clean card of knobs/switches) + Assign
// sections as collapsible cards (default collapsed). No page of dropdowns.
import { useState } from 'react';
import { useStore } from '../lib/store';
import ParamRenderer, { hexIndex } from './ParamRenderer';
import type { Param } from '../lib/api';
import LABELS from '../lib/paramLabels.json';

const ASSIGN_SECTIONS: { sec: number; title: string; count: number }[] = [
  { sec: 0x07, title: 'Footswitch Assign', count: 0x22 },
  { sec: 0x08, title: 'EXP Pedal Assign', count: 0x22 },
  { sec: 0x09, title: 'GAFC EXP1', count: 0x22 },
  { sec: 0x0a, title: 'GAFC EXP2', count: 0x22 },
  { sec: 0x0b, title: 'GAFC EXP3', count: 0x22 },
  { sec: 0x0c, title: 'GAFC-EX EXP1', count: 0x22 },
  { sec: 0x0d, title: 'GAFC-EX EXP2', count: 0x22 },
  { sec: 0x0e, title: 'GAFC-EX EXP3', count: 0x22 },
  { sec: 0x0f, title: 'Panel / Solo', count: 0x21 },
];

type AssignGroup = { idx: number; title: string; params: Param[] };

function groupParams(params: Param[], section: number): AssignGroup[] {
  const groups: AssignGroup[] = [];
  let current: AssignGroup | null = null;
  const list = params
    .filter((p) => p.section === section && p.kind === 'enum')
    .sort((a, b) => a.index - b.index);
  for (const p of list) {
    const key = `0x${section.toString(16).padStart(2, '0').toUpperCase()}:0x${p.index.toString(16).padStart(2, '0').toUpperCase()}`;
    const fromMap = (LABELS as Record<string, string>)[key];
    const name = fromMap ?? (p.label.replace(/[:\s]+$/, '') || hexIndex(p.index));
    if (p.index === 0 || (current && p.index - current.idx !== 1)) {
      current = { idx: p.index, title: name, params: [] };
      groups.push(current);
    }
    current!.params.push(p);
  }
  return groups;
}

function Collapsible({ title, subtitle, children }: { title: string; subtitle?: string; children: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <section className="rounded-xl border border-[#2c2c38] bg-[#12141a]">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center justify-between px-4 py-3 text-left"
      >
        <span className="text-[12px] font-semibold uppercase tracking-wider text-accent">{title}</span>
        <span className="flex items-center gap-2">
          {subtitle && <span className="text-[10px] text-amp-500">{subtitle}</span>}
          <span className="text-amp-400">{open ? '−' : '+'}</span>
        </span>
      </button>
      {open && <div className="border-t border-[#2c2c38] p-4">{children}</div>}
    </section>
  );
}

function AssignSection({ section, title }: { section: number; title: string }) {
  const { state } = useStore();
  const groups = groupParams(state.params, section);
  if (groups.length === 0) return null;

  return (
    <Collapsible title={title} subtitle={`section ${hexIndex(section)}`}>
      <div className="flex flex-col gap-4">
        {groups.map((g) => (
          <div key={g.idx}>
            <h4 className="mb-1.5 text-[10px] uppercase tracking-wide text-amp-400">{g.title}</h4>
            <div className="flex flex-wrap gap-4">
              {g.params.map((p) => (
                <ParamRenderer key={p.index} section={section} index={p.index} label={p.label.replace(/[:\s]+$/, '') || hexIndex(p.index)} />
              ))}
            </div>
          </div>
        ))}
      </div>
    </Collapsible>
  );
}

export default function SettingsDrawer({ onClose }: { onClose: () => void }) {
  const { state } = useStore();

  // Output: MASTER is in section 0x05 (0x70 patch level, 0x71 master key).
  // Panel section 0x06: chain (0x00-0x13), solo (0x14/0x15), contour (0x16/0x17/0x1a),
  // cab resonance (0x43), amp gain/volume panel (0x50-0x56), boosters (0x24-0x26).
  const master = state.params.filter((p) => p.section === 0x05 && (p.index === 0x70 || p.index === 0x71));
  const panel: { idx: number; label: string }[] = [
    { idx: 0x14, label: 'Solo' },
    { idx: 0x15, label: 'Solo Level' },
    { idx: 0x16, label: 'Contour' },
    { idx: 0x17, label: 'Contour Select' },
    { idx: 0x1a, label: 'Contour Freq Shift' },
    { idx: 0x43, label: 'Cab Resonance' },
  ];
  const ampPanel: { idx: number; label: string }[] = [
    { idx: 0x50, label: 'Amp Type' },
    { idx: 0x51, label: 'Gain' },
    { idx: 0x52, label: 'Volume' },
    { idx: 0x53, label: 'Bass' },
    { idx: 0x54, label: 'Middle' },
    { idx: 0x55, label: 'Treble' },
    { idx: 0x56, label: 'Presence' },
  ];

  return (
    <aside className="flex h-full w-[420px] shrink-0 flex-col border-l border-[#2c2c38] bg-[#0e1014]">
      <header className="flex items-center justify-between border-b border-[#2c2c38] px-4 py-3">
        <h2 className="text-sm font-semibold uppercase tracking-wider text-accent">Settings</h2>
        <button type="button" onClick={onClose} className="rounded-md border border-[#2c2c38] px-2 py-1 text-xs text-amp-300 hover:border-accent hover:text-amp-100" aria-label="Close settings">
          ✕
        </button>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        <div className="flex flex-col gap-3">
          <section className="rounded-xl border border-[#2c2c38] bg-[#12141a] p-3">
            <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-accent">Output</h3>
            <div className="flex flex-wrap gap-4">
              {master.map((p) => (
                <ParamRenderer key={p.index} section={0x05} index={p.index} label={p.index === 0x70 ? 'Master Volume' : 'Master Key'} />
              ))}
            </div>
            <div className="mt-3 grid grid-cols-2 gap-3">
              {panel.map(({ idx, label }) => (
                <ParamRenderer key={idx} section={0x06} index={idx} label={label} />
              ))}
            </div>
          </section>

          <section className="rounded-xl border border-[#2c2c38] bg-[#12141a] p-3">
            <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-accent">Send/Return &amp; Loop</h3>
            <div className="grid grid-cols-2 gap-3">
              <ParamRenderer section={0x05} index={0x62} label="S/R On/Off" />
              <ParamRenderer section={0x05} index={0x63} label="S/R Mode" />
              <ParamRenderer section={0x05} index={0x64} label="Send" />
              <ParamRenderer section={0x05} index={0x65} label="Return" />
              <ParamRenderer section={0x06} index={0x21} label="Loop Position" />
            </div>
          </section>

          <section className="rounded-xl border border-[#2c2c38] bg-[#12141a] p-3">
            <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-accent">Chain Position</h3>
            <div className="grid grid-cols-2 gap-3">
              <ParamRenderer section={0x06} index={0x19} label="EQ2 Position" />
              <ParamRenderer section={0x06} index={0x22} label="EQ Position" />
              <ParamRenderer section={0x06} index={0x23} label="PDL Position" />
            </div>
          </section>

          <section className="rounded-xl border border-[#2c2c38] bg-[#12141a] p-3">
            <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-accent">Panel Amp</h3>
            <div className="flex flex-wrap gap-4">
              {ampPanel.map(({ idx, label }) => (
                <ParamRenderer key={idx} section={0x06} index={idx} label={label} />
              ))}
            </div>
          </section>

          <div className="flex flex-col gap-3">
            <h3 className="text-[11px] font-semibold uppercase tracking-wider text-amp-400">Assignments</h3>
            {ASSIGN_SECTIONS.map((s) => (
              <AssignSection key={s.sec} section={s.sec} title={s.title} />
            ))}
          </div>
        </div>
      </div>
    </aside>
  );
}
