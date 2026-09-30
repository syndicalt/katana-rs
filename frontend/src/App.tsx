// Katana MK2 Editor — chain-first shell: top bar, board canvas, right-side
// editor/settings drawers, floating chat launcher. No tabs.
import { useEffect, useState } from 'react';
import { StoreProvider, useStore } from './lib/store';
import { ThemeProvider, useTheme } from './lib/theme';
import { api, type PortInfo } from './lib/api';
import Board from './components/Board';
import EditorDrawer from './components/EditorDrawer';
import SettingsDrawer from './components/SettingsDrawer';
import ChatLauncher from './components/ChatLauncher';

const BANKS = Array.from({ length: 50 }, (_, i) => i + 1);
const SLOTS = Array.from({ length: 9 }, (_, i) => i + 1);

function Led({ connected }: { connected: boolean }) {
  const color = connected ? 'bg-ok led-on' : 'bg-danger';
  return (
    <span
      className={`inline-block h-2.5 w-2.5 rounded-full ${color}`}
      aria-label={connected ? 'Connected' : 'Disconnected'}
    />
  );
}

function IconBtn({ children, onClick, title, disabled }: { children: React.ReactNode; onClick: () => void; title: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      disabled={disabled}
      className="rounded-md border border-[#2c2c38] px-2 py-1 text-xs text-amp-300 transition-colors hover:border-accent hover:text-amp-100 disabled:opacity-40"
    >
      {children}
    </button>
  );
}

function TopBar({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { state, connect, setBank, setSlot, syncToAmp, writeSlot, readPatch } = useStore();
  const { theme, setMode } = useTheme();
  const [inputs, setInputs] = useState<PortInfo[]>([]);
  const [outputs, setOutputs] = useState<PortInfo[]>([]);
  const [selIn, setSelIn] = useState<string>('');
  const [selOut, setSelOut] = useState<string>('');

  const loadPorts = async () => {
    try {
      const dto = await api.listPorts();
      setInputs(dto.inputs);
      setOutputs(dto.outputs);
      if (!selIn) setSelIn(dto.inputs.find((p) => p.is_katanaport)?.name ?? dto.inputs[0]?.name ?? '');
      if (!selOut) setSelOut(dto.outputs.find((p) => p.is_katanaport)?.name ?? dto.outputs[0]?.name ?? '');
    } catch {
      /* no MIDI stack outside Tauri */
    }
  };

  useEffect(() => {
    void loadPorts();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const doConnect = async () => {
    await connect(selIn || inputs[0]?.name || '', selOut || outputs[0]?.name || '');
  };

  return (
    <header className="flex h-14 shrink-0 items-center gap-3 border-b border-[#2c2c38] bg-amp-900 px-4">
      <div className="flex items-baseline gap-1.5">
        <span className="bg-gradient-to-r from-accent to-accent-dim bg-clip-text text-lg font-extrabold tracking-widest text-transparent">
          KATANA MK2
        </span>
        <span className="text-lg font-bold tracking-widest text-amp-500">EDITOR</span>
      </div>

      <div className="flex items-center gap-1.5">
        <Led connected={state.connected} />
        <span className="text-xs text-amp-400">{state.connected ? 'Connected' : state.reason ?? 'Not connected'}</span>
      </div>

      <div className="flex items-center gap-2">
        <select
          value={selIn}
          onChange={(e) => setSelIn(e.target.value)}
          aria-label="MIDI in port"
          className="max-w-44 rounded-md border border-[#2c2c38] bg-amp-800 px-2 py-1 text-xs text-amp-100 outline-none"
        >
          {inputs.length === 0 && <option value="">No MIDI in</option>}
          {inputs.map((p) => (
            <option key={p.name} value={p.name}>{p.name}</option>
          ))}
        </select>
        <select
          value={selOut}
          onChange={(e) => setSelOut(e.target.value)}
          aria-label="MIDI out port"
          className="max-w-44 rounded-md border border-[#2c2c38] bg-amp-800 px-2 py-1 text-xs text-amp-100 outline-none"
        >
          {outputs.length === 0 && <option value="">No MIDI out</option>}
          {outputs.map((p) => (
            <option key={p.name} value={p.name}>{p.name}</option>
          ))}
        </select>
        <button
          type="button"
          onClick={state.connected ? () => connect(selIn || '', selOut || '') : () => void doConnect()}
          className="rounded-md bg-accent px-3 py-1 text-xs font-semibold text-amp-950 hover:bg-accent-dim"
        >
          {state.connected ? 'Disconnect' : 'Connect'}
        </button>
        <IconBtn onClick={() => void loadPorts()} title="Refresh ports">⟳</IconBtn>
      </div>

      <div className="ml-auto flex items-center gap-2">
        <label className="flex items-center gap-1 text-[10px] text-amp-400">
          Bank
          <select
            value={state.bank}
            onChange={(e) => setBank(Number(e.target.value))}
            className="rounded-md border border-[#2c2c38] bg-amp-800 px-1.5 py-1 text-xs text-amp-100 outline-none"
          >
            {BANKS.map((b) => <option key={b} value={b}>{b}</option>)}
          </select>
        </label>
        <label className="flex items-center gap-1 text-[10px] text-amp-400">
          Slot
          <select
            value={state.slot}
            onChange={(e) => setSlot(Number(e.target.value))}
            className="rounded-md border border-[#2c2c38] bg-amp-800 px-1.5 py-1 text-xs text-amp-100 outline-none"
          >
            {SLOTS.map((s) => <option key={s} value={s}>{s}</option>)}
          </select>
        </label>
        {state.patchName && <span className="font-mono text-xs text-amp-200">{state.patchName}</span>}
        {state.dirty && <span className="text-amber" title="Unsaved changes">●</span>}
        <IconBtn onClick={() => void readPatch()} title="Read from amp">↓</IconBtn>
        <IconBtn onClick={() => void syncToAmp()} title="Sync to amp">↑</IconBtn>
        <IconBtn onClick={() => void writeSlot()} title="Write slot">💾</IconBtn>
        <IconBtn onClick={() => setMode(theme.mode === 'dark' ? 'light' : 'dark')} title="Toggle theme">{theme.mode === 'dark' ? '☀' : '☾'}</IconBtn>
        <IconBtn onClick={onOpenSettings} title="Output / Assign settings">⚙</IconBtn>
      </div>
    </header>
  );
}

function Shell() {
  const [stage, setStage] = useState<number | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);

  return (
    <div className="flex h-screen flex-col bg-amp-950 text-amp-100">
      <TopBar onOpenSettings={() => { setSettingsOpen(true); setStage(null); }} />
      <div className="flex min-h-0 flex-1">
        <main className="min-h-0 min-w-0 flex-1">
          <Board onSelect={(id) => { setStage(id); setSettingsOpen(false); }} />
        </main>
        {settingsOpen ? (
          <SettingsDrawer onClose={() => setSettingsOpen(false)} />
        ) : (
          <EditorDrawer chainId={stage} onClose={() => setStage(null)} />
        )}
      </div>
      <ChatLauncher />
    </div>
  );
}

export default function App() {
  return (
    <ThemeProvider>
      <StoreProvider>
        <Shell />
      </StoreProvider>
    </ThemeProvider>
  );
}
