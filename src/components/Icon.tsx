const STAR = "M12 3.6l2.55 5.2 5.75.83-4.16 4.05.98 5.72L12 16.7l-5.12 2.7.98-5.72L3.7 9.63l5.75-.83z";

const paths: Record<string, string> = {
  sd: "M7 3h8l4 4v13a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zm2 1v3m3-3v3m3-3v3",
  folder: "M3 6a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z",
  back: "M15 18l-6-6 6-6",
  gear: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zm7.4-3a7.4 7.4 0 0 0-.1-1.2l2-1.6-2-3.4-2.4 1a7.5 7.5 0 0 0-2-1.2L14.5 3h-4l-.4 2.6a7.5 7.5 0 0 0-2 1.2l-2.4-1-2 3.4 2 1.6a7.4 7.4 0 0 0 0 2.4l-2 1.6 2 3.4 2.4-1a7.5 7.5 0 0 0 2 1.2l.4 2.6h4l.4-2.6a7.5 7.5 0 0 0 2-1.2l2.4 1 2-3.4-2-1.6c.1-.4.1-.8.1-1.2z",
  grid: "M4 4h7v7H4zm9 0h7v7h-7zM4 13h7v7H4zm9 0h7v7h-7z",
  loupe: "M3 5h18v14H3z",
  compare: "M3 5h8v14H3zm10 0h8v14h-8z",
  check: "M5 12l5 5 9-10",
  x: "M6 6l12 12M18 6L6 18",
  send: "M4 12l16-8-6 16-2-7z",
  eject: "M12 5l7 8H5zM5 17h14v2H5z",
  sparkle: "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z",
  filter: "M4 5h16l-6 8v6l-4-2v-4z",
  trash: "M5 7h14M10 7V4h4v3m-7 0l1 13h8l1-13",
  info: "M12 8h.01M11 12h1v5h1M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z",
  video: "M3 6h12v12H3zm12 4l6-3v10l-6-3z",
  zoom: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zm5-2l5 5M11 8v6M8 11h6",
  plus: "M12 5v14M5 12h14",
  star: STAR,
  "star-line": STAR,
  play: "M8 5.5v13l10.5-6.5z",
  archive: "M3 5h18v4H3zm2 4v10h14V9m-9 4h4",
  keyboard: "M3 7h18v10H3zm4 3h.01M11 10h.01M15 10h.01M8 14h8",
  burst: "M8 8h12v12H8zM5 17V5h12",
  undo: "M9 14L4 9l5-5M4 9h10a6 6 0 0 1 0 12h-3",
};

const FILLED = new Set(["eject", "sparkle", "star", "play"]);

export function Icon({ name, size = 18, stroke = 1.8, className }: { name: keyof typeof paths | string; size?: number; stroke?: number; className?: string }) {
  const filled = FILLED.has(name);
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke={filled ? "none" : "currentColor"}
      strokeWidth={stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d={paths[name] ?? ""} />
    </svg>
  );
}
