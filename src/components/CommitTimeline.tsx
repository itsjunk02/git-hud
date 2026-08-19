import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent,
} from "react";

import type { CommitInfo } from "../services/tauri-ipc";

const ROW_H = 30;
const LANE_W = 16;
const PAD_X = 22;
const NODE_R = 5;
const LANE_COLORS = [
  "#34d399",
  "#60a5fa",
  "#f472b6",
  "#fbbf24",
  "#a78bfa",
  "#f87171",
  "#22d3ee",
  "#a3e635",
];

function relativeTime(seconds: number | null): string {
  if (!seconds) return "";
  const diff = Date.now() / 1000 - seconds;
  const units: [number, string][] = [
    [31536000, "y"],
    [2592000, "mo"],
    [604800, "w"],
    [86400, "d"],
    [3600, "h"],
    [60, "m"],
  ];
  for (const [s, label] of units) {
    const v = Math.floor(diff / s);
    if (v >= 1) return `${v}${label} ago`;
  }
  return "just now";
}

/**
 * High-performance commit/branch graph rendered on a DPR-aware HTML5 Canvas directly
 * from real `git2` data. Handles thousands of rows without per-node DOM. Hover + click
 * select a commit; selection/hover are painted by redraw.
 */
export function CommitTimeline({
  commits,
  selectedId,
  onSelect,
}: {
  commits: CommitInfo[];
  selectedId: string | null;
  onSelect: (commit: CommitInfo) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [width, setWidth] = useState(0);
  const [hovered, setHovered] = useState<number | null>(null);

  useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) setWidth(entry.contentRect.width);
    });
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    const height = Math.max(commits.length * ROW_H, ROW_H);

    canvas.width = Math.floor(width * dpr);
    canvas.height = Math.floor(height * dpr);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    ctx.font = "12px ui-sans-serif, system-ui, sans-serif";
    ctx.textBaseline = "middle";

    const maxLane = commits.reduce((m, c) => Math.max(m, c.lane), 0);
    const graphWidth = PAD_X * 2 + maxLane * LANE_W;
    const indexById = new Map<string, number>();
    commits.forEach((c, i) => indexById.set(c.id, i));

    const laneX = (lane: number) => PAD_X + lane * LANE_W;
    const rowY = (i: number) => i * ROW_H + ROW_H / 2;

    // Edges under nodes.
    ctx.lineWidth = 1.75;
    commits.forEach((c, i) => {
      const x = laneX(c.lane);
      const y = rowY(i);
      for (const pid of c.parent_ids) {
        const pi = indexById.get(pid);
        if (pi === undefined) continue;
        const px = laneX(commits[pi].lane);
        const py = rowY(pi);
        ctx.strokeStyle = LANE_COLORS[commits[pi].lane % LANE_COLORS.length];
        ctx.beginPath();
        ctx.moveTo(x, y);
        const midY = (y + py) / 2;
        ctx.bezierCurveTo(x, midY, px, midY, px, py);
        ctx.stroke();
      }
    });

    const textX = Math.max(graphWidth, PAD_X * 2) + 8;
    const timeW = 62;

    commits.forEach((c, i) => {
      const y = rowY(i);
      const x = laneX(c.lane);
      const isSel = c.id === selectedId;
      const isHover = i === hovered;

      if (isSel || isHover) {
        ctx.fillStyle = isSel ? "rgba(52,211,153,0.12)" : "rgba(255,255,255,0.04)";
        ctx.fillRect(0, i * ROW_H, width, ROW_H);
      }

      const color = LANE_COLORS[c.lane % LANE_COLORS.length];
      ctx.beginPath();
      ctx.arc(x, y, NODE_R, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();
      if (isSel) {
        ctx.lineWidth = 2;
        ctx.strokeStyle = "#ecfdf5";
        ctx.stroke();
      }

      // Short id.
      ctx.textAlign = "left";
      ctx.fillStyle = "#a1a1aa";
      ctx.fillText(c.short_id, textX, y);

      // Relative time, right-aligned.
      ctx.textAlign = "right";
      ctx.fillStyle = "#71717a";
      ctx.fillText(relativeTime(c.timestamp), width - 12, y);

      // Summary, clipped to available width.
      const summaryX = textX + 72;
      const maxSummaryW = width - summaryX - timeW - 24;
      ctx.save();
      ctx.beginPath();
      ctx.rect(summaryX, i * ROW_H, Math.max(maxSummaryW, 10), ROW_H);
      ctx.clip();
      ctx.textAlign = "left";
      ctx.fillStyle = isSel ? "#ecfdf5" : "#e4e4e7";
      ctx.fillText(c.summary || "(no message)", summaryX, y);
      ctx.restore();
    });
  }, [commits, width, hovered, selectedId]);

  useEffect(() => {
    draw();
  }, [draw]);

  const indexFromEvent = (e: MouseEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    return Math.floor((e.clientY - rect.top) / ROW_H);
  };

  if (commits.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-zinc-500">
        No commits to display.
      </div>
    );
  }

  return (
    <div ref={containerRef} className="h-full overflow-auto">
      <canvas
        ref={canvasRef}
        onMouseMove={(e) => {
          const i = indexFromEvent(e);
          setHovered(i >= 0 && i < commits.length ? i : null);
        }}
        onMouseLeave={() => setHovered(null)}
        onClick={(e) => {
          const i = indexFromEvent(e);
          if (i >= 0 && i < commits.length) onSelect(commits[i]);
        }}
        className="block cursor-pointer"
      />
    </div>
  );
}
