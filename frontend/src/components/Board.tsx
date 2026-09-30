// Board view: horizontal signal-chain canvas. Stage tiles with pedal artwork,
// reorderable via drag (HTML5) with arrow-key buttons as a fallback.
// The chain is section 0x06 bytes 0x00-0x13 on the amp; each byte is an
// effect ID. Reorder -> store.setChain(order) marks dirty + sends set_chain.
import { useEffect, useMemo, useState } from 'react';
import { useStore } from '../lib/store';
import { CHAIN_IDS } from '../lib/appConfig';
import { fxArtwork, stageArtwork, stageKind } from '../lib/stageMeta';
import PedalImage from './PedalImage';

// Default chain from default.syx (documented in CHAIN_DESIGN.md). Used only
// when the backend/demo params haven't populated the chain yet.
const DEFAULT_CHAIN = [
  0x11, 0x0b, 0x0f, 0x05, 0x06, 0x0d, 0x04, 0x02, 0x0a, 0x0c,
  0x01, 0x07, 0x08, 0x09, 0x12, 0x00, 0x03, 0x13, 0x0e, 0x10,
];

// ---------- Stage tile art ----------

function tileArt(chainId: number, typeRaw: number): string | undefined {
  const kind = stageKind(chainId);
  if (kind === 'fx') return fxArtwork(typeRaw);
  return stageArtwork(chainId);
}

function StageTileArt({ chainId, typeRaw }: { chainId: number; typeRaw: number }) {
  const id = tileArt(chainId, typeRaw);
  if (id) return <PedalImage id={id} width={86} />;
  const meta = CHAIN_IDS[chainId];
  const name = meta?.short ?? chainId.toString(16).toUpperCase().padStart(2, '0');
  return (
    <div className="flex h-[135px] w-[86px] items-center justify-center rounded-lg border border-[#2c2c38] bg-[#161922]">
      <span className="text-[10px] font-semibold tracking-widest text-amp-300">{name}</span>
    </div>
  );
}

export default function Board({ onSelect }: { onSelect: (chainId: number) => void }) {
  const { state, setChain } = useStore();
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);

  // Chain = params section 0x06 idx 0x00..0x13. Demo/fallback: all-zero means
  // the real patch chain wasn't loaded; substitute the documented default.
  const paramChain = useMemo(() => {
    const idx = state.params.filter((p) => p.section === 0x06 && p.index <= 0x13).sort((a, b) => a.index - b.index);
    if (idx.length === 20) return idx.map((p) => p.raw);
    return null;
  }, [state.params]);
  const chain = paramChain && paramChain.some((v) => v !== 0) ? paramChain : DEFAULT_CHAIN;

  const [order, setOrder] = useState<number[]>(chain);
  useEffect(() => {
    setOrder(chain);
  }, [chain]);

  const commit = (next: number[]) => {
    setOrder(next);
    setChain(next);
  };

  const move = (from: number, to: number) => {
    if (from === to || from < 0 || to < 0 || from >= order.length || to >= order.length) return;
    const next = [...order];
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    commit(next);
  };

  const onDrop = (to: number) => {
    if (dragging !== null) move(dragging, to);
    setDragging(null);
  };

  // Resolve the type byte for FX slots from params (FX1: sec 0x01 idx 0x01; FX2: sec 0x03 idx 0x01).
  const fxType = (chainId: number): number => {
    if (chainId === 0x05) {
      const p = state.params.find((q) => q.section === 0x01 && q.index === 0x01);
      return p?.raw ?? 0;
    }
    if (chainId === 0x06) {
      const p = state.params.find((q) => q.section === 0x03 && q.index === 0x01);
      return p?.raw ?? 0;
    }
    return 0;
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between px-4 pb-2">
        <div>
          <h1 className="text-sm font-semibold uppercase tracking-wider text-amp-200">Signal Chain</h1>
          <p className="text-[11px] text-amp-400">Drag tiles to reorder · click to edit</p>
        </div>
        <span className="text-[11px] text-amp-500">CH_A / CH_B carry the amp · FX1/FX2 hold mod·fx types</span>
      </header>
      <div className="min-h-0 flex-1 overflow-x-auto overflow-y-hidden px-4 pb-4">
        <div className="flex h-full min-w-max items-center gap-2">
          {order.map((chainId, i) => {
            const meta = CHAIN_IDS[chainId];
            const active = selectedId === chainId || (selectedId === null && i === 0);
            return (
              <div
                key={`${chainId}-${i}`}
                draggable
                onDragStart={(e) => {
                  setDragging(i);
                  e.dataTransfer.effectAllowed = 'move';
                }}
                onDragOver={(e) => {
                  e.preventDefault();
                  e.dataTransfer.dropEffect = 'move';
                }}
                onDrop={() => onDrop(i)}
                onDragEnd={() => setDragging(null)}
                onClick={() => {
                  setSelectedId(chainId);
                  onSelect(chainId);
                }}
                className={`group flex w-[118px] cursor-pointer flex-col items-center gap-1.5 rounded-xl border p-2 transition-colors ${
                  active ? 'border-accent bg-[#161922]' : 'border-[#2c2c38] bg-[#12141a] hover:border-[#3b4d65]'
                } ${dragging === i ? 'opacity-40' : ''}`}
              >
                <span className="self-start text-[9px] font-mono text-amp-500">{String(i).padStart(2, '0')}</span>
                <StageTileArt chainId={chainId} typeRaw={fxType(chainId)} />
                <span className="max-w-full truncate text-[10px] font-medium text-amp-200">{meta?.name ?? '?'}</span>
                <span className="max-w-full truncate text-[9px] text-amp-500">{meta?.short}</span>
                {dragging === null && (
                  <div className="flex gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                    <button title="Move left" className="rounded border border-[#2c2c38] px-1 text-[10px] text-amp-300 hover:text-amp-100" onClick={(e) => { e.stopPropagation(); move(i, i - 1); }}>◀</button>
                    <button title="Move right" className="rounded border border-[#2c2c38] px-1 text-[10px] text-amp-300 hover:text-amp-100" onClick={(e) => { e.stopPropagation(); move(i, i + 1); }}>▶</button>
                  </div>
                )}
              </div>
            );
          })}
          <div className="w-4 shrink-0" />
        </div>
      </div>
    </div>
  );
}
