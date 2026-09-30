// Floating chat launcher (bottom-right) with slide-up drawer. Content behind
// keeps scrolling; the drawer overlays it. No chat tab anywhere.
import { useEffect, useRef, useState } from 'react';
import { useStore } from '../lib/store';

export default function ChatLauncher() {
  const { chat, chatPending, chatError, sendChat, applyAgent } = useStore();
  const [open, setOpen] = useState(false);
  const [input, setInput] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [chat, chatPending, open]);

  const submit = () => {
    if (!input.trim() || chatPending) return;
    void sendChat(input);
    setInput('');
  };

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-label={open ? 'Close chat' : 'Open chat'}
        className="fixed bottom-4 right-4 z-40 flex h-14 w-14 items-center justify-center rounded-full border border-accent-dim bg-accent text-amp-950 shadow-lg shadow-black/50 transition-transform hover:scale-105 focus:outline-none"
        style={{ boxShadow: '0 4px 18px rgba(0,0,0,0.5), 0 0 12px rgba(92,166,224,0.35)' }}
      >
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
        </svg>
        {chat.length > 0 && (
          <span className="absolute -top-1 -right-1 flex h-5 min-w-5 items-center justify-center rounded-full bg-ok px-1 text-[10px] font-bold text-amp-950">
            {chat.length}
          </span>
        )}
      </button>

      {open && (
        <div className="slide-in fixed bottom-22 right-4 z-40 flex h-[70vh] max-h-[560px] w-[380px] max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-2xl border border-[#3b4d65] bg-[#12141a] shadow-2xl shadow-black/70">
          <header className="flex items-center justify-between border-b border-[#2c2c38] px-4 py-3">
            <h3 className="text-sm font-semibold uppercase tracking-wider text-ok">Agent chat</h3>
            <div className="flex items-center gap-2">
              {!chatPending && chat.length > 0 && (
                <button
                  type="button"
                  onClick={() => void applyAgent()}
                  className="rounded-md border border-ok/40 px-2 py-1 text-[11px] font-medium text-ok hover:bg-ok/10"
                >
                  ▶ Apply to amp
                </button>
              )}
              <button type="button" onClick={() => setOpen(false)} className="rounded-md border border-[#2c2c38] px-2 py-1 text-xs text-amp-300 hover:text-amp-100" aria-label="Close chat">
                ✕
              </button>
            </div>
          </header>

          <div ref={scrollRef} className="min-h-0 flex-1 space-y-3 overflow-y-auto p-4">
            {chat.length === 0 && !chatPending && (
              <p className="text-sm text-amp-400">Ask the agent to replicate a band/song/style…</p>
            )}
            {chat.map((m, i) => (
              <div key={i} className={`flex ${m.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                <div
                  className={`max-w-[80%] rounded-xl px-4 py-2.5 text-sm whitespace-pre-wrap ${
                    m.role === 'user' ? 'bg-[#1d334a] text-amp-100' : 'bg-[#1e2e24] text-amp-100'
                  }`}
                >
                  <div className={`mb-0.5 text-[11px] font-semibold ${m.role === 'user' ? 'text-accent' : 'text-ok'}`}>
                    {m.role === 'user' ? 'You' : 'Agent'}
                  </div>
                  {m.content}
                </div>
              </div>
            ))}
            {chatPending && (
              <div className="flex justify-start">
                <div className="rounded-xl bg-[#1e2e24] px-4 py-3">
                  <span className="typing-dots">
                    <span />
                    <span />
                    <span />
                  </span>
                </div>
              </div>
            )}
          </div>

          {chatError && <p className="px-4 pb-1 text-xs text-danger">{chatError}</p>}

          <div className="flex gap-2 border-t border-[#2c2c38] p-3">
            <input
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && submit()}
              placeholder='Ask about a band/song/style…'
              className="min-w-0 flex-1 rounded-lg border border-[#2c2c38] bg-amp-800 px-3 py-2 text-sm text-amp-100 outline-none placeholder:text-amp-500 focus:border-accent"
            />
            <button
              type="button"
              onClick={submit}
              disabled={chatPending || !input.trim()}
              className="rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-amp-950 transition-colors hover:bg-accent-dim disabled:opacity-40"
            >
              Send
            </button>
          </div>
        </div>
      )}
    </>
  );
}
