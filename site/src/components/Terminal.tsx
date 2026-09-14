import { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

interface Line {
  kind: "prompt" | "info" | "match" | "empty";
  text: string;
}

const scripts: Record<string, Line[]> = {
  pubkey: [
    { kind: "prompt", text: "$ find --pubkey 0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798" },
    { kind: "info",   text: "2025-01-15T12:42:18Z INFO  find::config  parsed pubkey (33 bytes, SEC1 compressed)" },
    { kind: "info",   text: "2025-01-15T12:42:18Z INFO  find::search  generated 512 variants (once-lock interned)" },
    { kind: "info",   text: "2025-01-15T12:42:18Z INFO  find::orchestrator  sweep starting  batch_size=32  variants=512" },
    { kind: "info",   text: "2025-01-15T12:42:19Z INFO  find::orchestrator  TRILLION boundary crossed at j=1_000_000_000_000" },
    { kind: "info",   text: "2025-01-15T12:42:21Z INFO  find::orchestrator  sweep complete  elapsed=2.84s  probed=1_247_823_104" },
    { kind: "match",  text: "MATCH DISCOVERED via V=0x4b… via variant index 217 at j=47_318_291" },
    { kind: "match",  text: "Candidates (d = V ± j mod n):" },
    { kind: "match",  text: "  d₀ = 0x4bf7c2f0d3e8a91b1c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b" },
    { kind: "match",  text: "  d₁ = 0x4bf7c2f0d3e8a91b1c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6a" },
    { kind: "info",   text: "checkpoint written to data/checkpoint.json (atomic, fsync)" },
    { kind: "empty",  text: "" },
  ],
  address: [
    { kind: "prompt", text: "$ find --address 1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa \\" },
    { kind: "prompt", text: "        --from 1 --to 100000000" },
    { kind: "info",   text: "2025-01-15T12:42:18Z INFO  find::config  address mode  target=1A1zP1eP5…  range=[1, 100_000_000]" },
    { kind: "info",   text: "2025-01-15T12:42:18Z INFO  find::search  hash40 = 0xa1b2c3d4…  variant set pre-filtered" },
    { kind: "info",   text: "2025-01-15T12:42:18Z INFO  find::orchestrator  sweep starting  parallel=true  cores=10" },
    { kind: "info",   text: "2025-01-15T12:42:24Z INFO  find::orchestrator  boundary at j=10_000_000 (rayon early-exit enabled)" },
    { kind: "info",   text: "2025-01-15T12:42:28Z INFO  find::orchestrator  sweep complete  elapsed=9.74s  probed=99_999_999" },
    { kind: "info",   text: "no match in range [1, 100_000_000]  (genuine miss — expected for this target)" },
    { kind: "empty",  text: "" },
  ],
  bench: [
    { kind: "prompt", text: "$ cargo bench --bench bench" },
    { kind: "info",   text: "plus_g_chain/chain_32_plus_g            time:   [17.99 µs 18.10 µs 18.21 µs]" },
    { kind: "info",   text: "plus_g_chain/naive_32_scalar_muls       time:   [423.7 µs 425.1 µs 426.6 µs]" },
    { kind: "info",   text: "match_x/aligned_slice                   time:   [ 31.4 ns  31.6 ns  31.9 ns]" },
    { kind: "info",   text: "end_to_end_small_scalar_12345/10M      time:   [1.881 ms 1.886 ms 1.892 ms]" },
    { kind: "info",   text: "batch_normalization/32_points           time:   [38.2 µs 38.5 µs 38.8 µs]" },
    { kind: "match",  text: "Speedup vs pre-precomputed-tables: 2.80× (end-to-end)" },
    { kind: "empty",  text: "" },
  ],
};

const tabs = [
  { id: "pubkey", label: "find --pubkey" },
  { id: "address", label: "find --address" },
  { id: "bench", label: "cargo bench" },
] as const;

type TabId = typeof tabs[number]["id"];

export default function Terminal() {
  const [tab, setTab] = useState<TabId>("pubkey");
  const [visible, setVisible] = useState(0);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setVisible(0);
    const lines = scripts[tab];
    if (!lines.length) return;
    const reduced =
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduced) {
      setVisible(lines.length);
      return;
    }
    const id = setInterval(() => {
      setVisible((v) => {
        if (v >= lines.length) {
          clearInterval(id);
          return v;
        }
        return v + 1;
      });
    }, 240);
    return () => clearInterval(id);
  }, [tab]);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [visible, tab]);

  const lines = scripts[tab];

  return (
    <div className="card overflow-hidden !bg-[var(--terminal-bg)]">
      <div className="flex items-center justify-between border-b border-[var(--line)] px-3 py-2.5">
        <div className="flex items-center gap-2">
          <span className="h-2.5 w-2.5 rounded-full bg-[#ff5f57]/80"></span>
          <span className="h-2.5 w-2.5 rounded-full bg-[#febc2e]/80"></span>
          <span className="h-2.5 w-2.5 rounded-full bg-[#28c840]/80"></span>
          <span className="ml-2 font-mono text-[0.72rem] tracking-tight text-[var(--fg-subtle)]">
            ~/find — zsh — 92×24
          </span>
        </div>
        <div className="flex items-center gap-1 rounded-full border border-[var(--line)] bg-[var(--bg-sunken)] p-0.5">
          {tabs.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={`relative rounded-full px-3 py-1 font-mono text-[0.7rem] transition ${
                tab === t.id
                  ? "bg-[var(--fg)] text-[var(--bg)]"
                  : "text-[var(--fg-muted)] hover:text-[var(--fg)]"
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>
      </div>

      <div
        ref={scrollRef}
        className="relative max-h-[24rem] overflow-y-auto bg-[var(--terminal-bg)] p-5 font-mono text-[0.78rem] leading-relaxed text-[var(--terminal-fg)]"
      >
        <AnimatePresence initial={false}>
          {lines.slice(0, visible).map((line, i) => (
            <motion.div
              key={`${tab}-${i}`}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.32, ease: [0.16, 1, 0.3, 1] }}
              className={
                line.kind === "prompt"
                  ? "text-[var(--fg)]"
                  : line.kind === "match"
                  ? "text-[color:var(--color-brand-500)]"
                  : "text-[var(--fg-muted)]"
              }
            >
              {line.kind === "prompt" ? (
                <span>
                  <span className="mr-1 text-[color:var(--color-brand-500)]">›</span>
                  {line.text.replace(/^\$ /, "")}
                </span>
              ) : line.text ? (
                <span className="whitespace-pre">{line.text}</span>
              ) : (
                <span>&nbsp;</span>
              )}
            </motion.div>
          ))}
        </AnimatePresence>

        {visible >= lines.length && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.2 }}
            className="mt-1 inline-flex items-center gap-1.5 text-[var(--fg-muted)]"
          >
            <span className="text-[color:var(--color-brand-500)]">›</span>
            <span className="h-3 w-1.5 animate-pulse bg-[color:var(--color-brand-500)]"></span>
          </motion.div>
        )}
      </div>
    </div>
  );
}