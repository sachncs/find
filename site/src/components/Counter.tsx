import { useEffect, useRef, useState } from "react";

interface Props {
  value: number;
  suffix?: string;
  prefix?: string;
  decimals?: number;
  duration?: number;
  className?: string;
}

function easeOutExpo(t: number) {
  return t === 1 ? 1 : 1 - Math.pow(2, -10 * t);
}

/**
 * Renders the final value in SSR/no-JS contexts.
 * On hydration, animates from 0 up to `value` for the cinematic reveal.
 * Respects prefers-reduced-motion.
 */
export default function Counter({
  value,
  suffix = "",
  prefix = "",
  decimals = 0,
  duration = 1400,
  className,
}: Props) {
  const [hydrated, setHydrated] = useState(false);
  const [display, setDisplay] = useState(value);
  const ref = useRef<HTMLSpanElement>(null);
  const animRef = useRef<number | null>(null);

  useEffect(() => {
    setHydrated(true);
    const reduced =
      window.matchMedia &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduced) {
      setDisplay(value);
      return;
    }

    let cancelled = false;
    setDisplay(0);
    const t0 = performance.now();
    const tick = (now: number) => {
      if (cancelled) return;
      const p = Math.min(1, (now - t0) / duration);
      const v = value * easeOutExpo(p);
      setDisplay(p >= 1 ? value : v);
      if (p < 1) animRef.current = requestAnimationFrame(tick);
    };
    animRef.current = requestAnimationFrame(tick);
    return () => {
      cancelled = true;
      if (animRef.current) cancelAnimationFrame(animRef.current);
    };
  }, [value, duration]);

  const formatted = display.toLocaleString(undefined, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });

  return (
    <span
      ref={ref}
      className={className}
      style={{
        fontVariantNumeric: "tabular-nums",
        opacity: hydrated ? 1 : 1,
      }}
    >
      {prefix}
      {formatted}
      {suffix}
    </span>
  );
}