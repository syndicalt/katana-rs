// Editor drawer: contextual controls for the selected chain stage.
// Params are filtered per stage kind (fxMap-ranges for FX types, fixed
// curated blocks elsewhere) so only relevant controls ever appear.
import { useStore } from '../lib/store';
import { CHAIN_IDS, AMP } from '../lib/appConfig';
import { fxParamRanges, stageKind } from '../lib/stageMeta';
import ParamRenderer, { hexIndex } from './ParamRenderer';
import type { Param } from '../lib/api';

const BANDS = ['31', '62', '125', '250', '500', '1k', '2k', '4k', '8k', '16k'];

// ---------- helpers ----------

function paramsIn(stateParams: Param[], section: number, lo: number, hi: number): Param[] {
  return stateParams
    .filter((p) => p.section === section && p.index >= lo && p.index <= hi)
    .sort((a, b) => a.index - b.index);
}

function ControlGrid({ section, items, fader = false }: { section: number; items: number[]; fader?: boolean }) {
  return (
    <div className="flex flex-wrap gap-4">
      {items.map((idx) => (
        <ParamRenderer key={idx} section={section} index={idx}  fader={fader} />
      ))}
    </div>
  );
}

function SectionCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-xl border border-[#2c2c38] bg-[#12141a] p-3">
      <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-accent">{title}</h3>
      {children}
    </section>
  );
}

function EmptyNote({ text }: { text: string }) {
  return <p className="text-xs text-amp-400">{text}</p>;
}

// ---------- Amp stage ----------

function AmpPanel() {
  // Section 0x00 (PRE). Curated universal tone controls; constants from appConfig.AMP.
  const tone: { idx: number; label: string }[] = [
    { idx: AMP.gain.index, label: 'Gain' },
    { idx: AMP.bass.index, label: 'Bass' },
    { idx: AMP.mid.index, label: 'Mid' },
    { idx: AMP.treble.index, label: 'Treble' },
    { idx: AMP.presence.index, label: 'Presence' },
    { idx: AMP.volume.index, label: 'Volume' },
  ];
  const switches: { idx: number; label: string }[] = [
    { idx: AMP.power_amp.index, label: 'Power Amp In' },
    { idx: AMP.variation.index, label: 'Variation' },
    { idx: AMP.gain_switch.index, label: 'Gain Switch' },
    { idx: AMP.solo.index, label: 'Solo' },
  ];
  return (
    <div className="flex flex-col gap-3">
      <SectionCard title="Amp Type">
        <ParamRenderer section={0} index={AMP.type.index} label="Amp Type" />
      </SectionCard>
      <SectionCard title="Tone">
        <div className="flex flex-wrap gap-4">
          {tone.map(({ idx, label }) => (
            <ParamRenderer key={idx} section={0} index={idx} label={label} />
          ))}
        </div>
        <div className="mt-3 flex flex-wrap gap-4">
          {switches.map(({ idx, label }) => (
            <ParamRenderer key={idx} section={0} index={idx} label={label} />
          ))}
        </div>
      </SectionCard>
      <SectionCard title="Booster">
        <div className="flex flex-wrap items-end gap-4">
          <ParamRenderer section={0} index={0x10} label="On/Off" />
          <ParamRenderer section={0} index={0x11} label="Type" />
        </div>
        <div className="mt-3 flex flex-wrap gap-4">
          {[0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18].map((idx) => (
            <ParamRenderer key={idx} section={0} index={idx}  />
          ))}
        </div>
      </SectionCard>
    </div>
  );
}

// ---------- FX stage (FX1/FX2) ----------

function FxPanel({ mainSection, extSection }: { mainSection: number; extSection: number }) {
  const { state } = useStore();
  const typeParam = state.params.find((p) => p.section === mainSection && p.index === 0x01);
  const raw = typeParam?.raw ?? 0;
  const { main, ext } = fxParamRanges(raw);

  const mainItems = main ? paramsIn(state.params, mainSection, main[0], main[1]).map((p) => p.index) : [];
  const extItems = ext ? paramsIn(state.params, extSection, ext[0], ext[1]).map((p) => p.index) : [];

  return (
    <div className="flex flex-col gap-3">
      <SectionCard title="Effect">
        <div className="flex flex-wrap items-end gap-4">
          <ParamRenderer section={mainSection} index={0x00} label="On/Off" />
          <ParamRenderer section={mainSection} index={0x01} label="Type" />
        </div>
      </SectionCard>
      {mainItems.length > 0 && (
        <SectionCard title="Parameters">
          <div className="flex flex-wrap gap-4">
            {mainItems.map((idx) => (
              <ParamRenderer key={idx} section={mainSection} index={idx}  />
            ))}
          </div>
        </SectionCard>
      )}
      {extItems.length > 0 && (
        <SectionCard title="Extended">
          <div className="flex flex-wrap gap-4">
            {extItems.map((idx) => (
              <ParamRenderer key={idx} section={extSection} index={idx}  />
            ))}
          </div>
        </SectionCard>
      )}
      {mainItems.length === 0 && extItems.length === 0 && (
        <p className="text-xs text-amp-400">This effect has no additional parameters.</p>
      )}
    </div>
  );
}

// ---------- Delay / Reverb ----------

const DD_BASE = [0x00, 0x01, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0b, 0x0c, 0x0d, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19];

function DelayPanel({ base }: { base: number }) {
  const { state } = useStore();
  const items = DD_BASE.map((off) => base + off).filter((idx) =>
    state.params.some((p) => p.section === 0x05 && p.index === idx),
  );
  return (
    <div className="flex flex-col gap-3">
      <SectionCard title="Delay">
        <div className="flex flex-wrap items-end gap-4">
          <ParamRenderer section={0x05} index={base + 0x00} label="On/Off" />
          <ParamRenderer section={0x05} index={base + 0x01} label="Type" />
        </div>
      </SectionCard>
      <SectionCard title="Parameters">
        <div className="flex flex-wrap gap-4">
          {items.slice(2).map((idx) => (
            <ParamRenderer key={idx} section={0x05} index={idx}  />
          ))}
        </div>
      </SectionCard>
    </div>
  );
}

function ReverbPanel() {
  const { state } = useStore();
  const items = paramsIn(state.params, 0x05, 0x42, 0x4b).map((p) => p.index);
  return (
    <div className="flex flex-col gap-3">
      <SectionCard title="Reverb">
        <div className="flex flex-wrap items-end gap-4">
          <ParamRenderer section={0x05} index={0x40} label="On/Off" />
          <ParamRenderer section={0x05} index={0x41} label="Type" />
        </div>
      </SectionCard>
      <SectionCard title="Parameters">
        <div className="flex flex-wrap gap-4">
          {items.map((idx) => (
            <ParamRenderer key={idx} section={0x05} index={idx}  />
          ))}
        </div>
      </SectionCard>
    </div>
  );
}

// ---------- EQ ----------

function EqPanel({ peqBase, geqBase }: { peqBase: number; geqBase: number }) {
  const peq = Array.from({ length: 13 }, (_, k) => peqBase + k);
  const geq = BANDS.map((_, k) => geqBase + k);
  return (
    <div className="flex flex-col gap-3">
      <SectionCard title="Parametric EQ">
        <div className="flex flex-wrap gap-4">
          {peq.map((idx) => (
            <ParamRenderer key={idx} section={0} index={idx}  />
          ))}
        </div>
      </SectionCard>
      <SectionCard title="Graphic EQ">
        <div className="flex flex-wrap gap-4">
          {geq.map((idx, k) => (
            <ParamRenderer key={idx} section={0} index={idx} label={BANDS[k]} fader />
          ))}
        </div>
        <div className="mt-3 flex flex-wrap gap-4">
          <ParamRenderer section={0} index={geqBase + 10} label="Level" />
        </div>
      </SectionCard>
    </div>
  );
}

// ---------- Other stages ----------

function CsPanel() {
  const { state } = useStore();
  const items = paramsIn(state.params, 0x01, 0x16, 0x1a).map((p) => p.index);
  return <ControlGrid section={0x01} items={items} />;
}

function LpPanel() {
  const { state } = useStore();
  const items = paramsIn(state.params, 0x01, 0x1c, 0x20).map((p) => p.index);
  return (
    <div className="flex flex-col gap-3">
      <SectionCard title="Limiter">
        <div className="flex flex-wrap items-end gap-4">
          <ParamRenderer section={0x01} index={0x1b} label="Type" />
        </div>
        <div className="mt-3 flex flex-wrap gap-4">
          {items.map((idx) => (
            <ParamRenderer key={idx} section={0x01} index={idx}  />
          ))}
        </div>
      </SectionCard>
    </div>
  );
}

function OdPanel() {
  const { state } = useStore();
  const items = paramsIn(state.params, 0x00, 0x10, 0x1e).map((p) => p.index);
  return (
    <div className="flex flex-col gap-3">
      <SectionCard title="Overdrive / Booster">
        <div className="flex flex-wrap items-end gap-4">
          <ParamRenderer section={0} index={0x10} label="On/Off" />
          <ParamRenderer section={0} index={0x11} label="Type" />
        </div>
        <div className="mt-3 flex flex-wrap gap-4">
          {items.slice(2).map((idx) => (
            <ParamRenderer key={idx} section={0} index={idx}  />
          ))}
        </div>
      </SectionCard>
    </div>
  );
}

function PdlPanel() {
  const { state } = useStore();
  const main = paramsIn(state.params, 0x05, 0x50, 0x51).map((p) => p.index);
  const wah = paramsIn(state.params, 0x05, 0x52, 0x57).map((p) => p.index);
  const bend = paramsIn(state.params, 0x05, 0x58, 0x5b).map((p) => p.index);
  const wah95 = paramsIn(state.params, 0x05, 0x5c, 0x60).map((p) => p.index);
  return (
    <div className="flex flex-col gap-3">
      <SectionCard title="Pedal FX">
        <div className="flex flex-wrap items-end gap-4">
          {main.map((idx) => (
            <ParamRenderer key={idx} section={0x05} index={idx} label={idx === 0x50 ? 'On/Off' : 'Type'} />
          ))}
        </div>
      </SectionCard>
      {wah.length > 0 && (
        <SectionCard title="WAH">
          <ControlGrid section={0x05} items={wah} />
        </SectionCard>
      )}
      {bend.length > 0 && (
        <SectionCard title="Pedal Bend">
          <ControlGrid section={0x05} items={bend} />
        </SectionCard>
      )}
      {wah95.length > 0 && (
        <SectionCard title="WAH 95E">
          <ControlGrid section={0x05} items={wah95} />
        </SectionCard>
      )}
    </div>
  );
}

function FvPanel() {
  return (
    <SectionCard title="Foot Volume">
      <ParamRenderer section={0x05} index={0x61} label="Level" />
    </SectionCard>
  );
}

function NsPanel() {
  return (
    <SectionCard title="Noise Suppressor">
      <div className="flex flex-wrap items-end gap-4">
        <ParamRenderer section={0x05} index={0x66} label="On/Off" />
      </div>
      <div className="mt-3 flex flex-wrap gap-4">
        <ParamRenderer section={0x05} index={0x67} label="Threshold" />
        <ParamRenderer section={0x05} index={0x68} label="Release" />
      </div>
    </SectionCard>
  );
}

function CabPanel() {
  return (
    <SectionCard title="Cabinet">
      <div className="flex flex-wrap gap-4">
        <ParamRenderer section={0x06} index={0x43} label="Cab Resonance" />
      </div>
    </SectionCard>
  );
}

// ---------- Drawer ----------

export default function EditorDrawer({ chainId, onClose }: { chainId: number | null; onClose: () => void }) {
  const meta = chainId !== null ? CHAIN_IDS[chainId] : undefined;
  const kind = chainId !== null ? stageKind(chainId) : 'static';

  if (chainId === null) {
    return (
      <aside className="flex h-full w-[380px] shrink-0 flex-col border-l border-[#2c2c38] bg-[#0e1014]">
        <div className="p-4">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-amp-300">Editor</h2>
          <p className="mt-1 text-xs text-amp-400">Select a stage on the board to edit it.</p>
        </div>
      </aside>
    );
  }

  let body: React.ReactNode = null;
  if (kind === 'amp') body = <AmpPanel />;
  else if (kind === 'fx') body = chainId === 0x05 ? <FxPanel mainSection={0x01} extSection={0x02} /> : <FxPanel mainSection={0x03} extSection={0x04} />;
  else if (kind === 'delay') body = chainId === 0x07 ? <DelayPanel base={0x00} /> : <DelayPanel base={0x20} />;
  else if (kind === 'reverb') body = <ReverbPanel />;
  else if (kind === 'eq') body = chainId === 0x04 ? <EqPanel peqBase={0x40} geqBase={0x4d} /> : <EqPanel peqBase={0x60} geqBase={0x6d} />;
  else if (kind === 'cs') body = <CsPanel />;
  else if (kind === 'lp') body = <LpPanel />;
  else if (kind === 'od') body = <OdPanel />;
  else if (kind === 'pdl') body = <PdlPanel />;
  else if (kind === 'fv') body = <FvPanel />;
  else if (kind === 'ns') body = <NsPanel />;
  else if (kind === 'cab') body = <CabPanel />;
  else body = <EmptyNote text="Fixed stage — no dedicated parameters." />;

  return (
    <aside className="flex h-full w-[380px] shrink-0 flex-col border-l border-[#2c2c38] bg-[#0e1014]">
      <header className="flex items-center justify-between border-b border-[#2c2c38] px-4 py-3">
        <div>
          <h2 className="text-sm font-semibold uppercase tracking-wider text-accent">{meta?.name ?? 'Stage'}</h2>
          <p className="text-[10px] text-amp-500">chain id {hexIndex(chainId)} · {meta?.short}</p>
        </div>
        <button type="button" onClick={onClose} className="rounded-md border border-[#2c2c38] px-2 py-1 text-xs text-amp-300 hover:border-accent hover:text-amp-100" aria-label="Close editor">
          ✕
        </button>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto p-4">{body}</div>
    </aside>
  );
}
