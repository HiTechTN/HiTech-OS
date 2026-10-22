interface IconProps {
  className?: string;
}

function base(className: string | undefined) {
  return {
    className: className ?? "h-4 w-4",
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.7,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
}

export function IconTerminal({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="m4 17 6-6-6-6" />
      <path d="M12 19h8" />
    </svg>
  );
}

export function IconActivity({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="M3 12h4l3 8 4-16 3 8h4" />
    </svg>
  );
}

export function IconShield({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="M12 3 4.5 6v5.5c0 4.5 3 8 7.5 9.5 4.5-1.5 7.5-5 7.5-9.5V6L12 3Z" />
      <path d="m9 12 2 2 4-4" />
    </svg>
  );
}

export function IconWrench({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="M14.7 6.3a4.5 4.5 0 0 0 6 6L13 20a2.8 2.8 0 0 1-4-4l7.7-7.7Z" />
      <path d="m5 5 3 3" />
    </svg>
  );
}

export function IconScroll({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="M6 4h11a2 2 0 0 1 2 2v11a3 3 0 0 1-3 3H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z" />
      <path d="M8 8h8M8 12h8M8 16h5" />
    </svg>
  );
}

export function IconSend({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="M4 12 20 4l-4 16-4-7-8-1Z" />
    </svg>
  );
}

export function IconCheck({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="m4 12 5 5L20 6" />
    </svg>
  );
}

export function IconX({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="M6 6l12 12M18 6 6 18" />
    </svg>
  );
}

export function IconRefresh({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="M20 11a8 8 0 1 0-2.3 5.7" />
      <path d="M20 5v6h-6" />
    </svg>
  );
}

export function IconAlert({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="M12 4 3 20h18L12 4Z" />
      <path d="M12 10v4M12 17h.01" />
    </svg>
  );
}

export function IconCpu({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <rect x="7" y="7" width="10" height="10" rx="2" />
      <path d="M9 3v2M15 3v2M9 19v2M15 19v2M3 9h2M3 15h2M19 9h2M19 15h2" />
    </svg>
  );
}

export function IconMemory({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="M4 8h16v9H4z" />
      <path d="M7 8V5M12 8V5M17 8V5M8 17v3M16 17v3" />
    </svg>
  );
}

export function IconServer({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <rect x="3" y="4" width="18" height="7" rx="2" />
      <rect x="3" y="13" width="18" height="7" rx="2" />
      <path d="M7 7.5h.01M7 16.5h.01" />
    </svg>
  );
}

export function IconChevron({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="m9 6 6 6-6 6" />
    </svg>
  );
}

export function IconHex({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <path d="M12 2.5 20 7v10l-8 4.5L4 17V7l8-4.5Z" />
      <path d="M12 7.5 16 10v4l-4 2.5L8 14v-4l4-2.5Z" />
    </svg>
  );
}

export function IconLock({ className }: IconProps) {
  return (
    <svg {...base(className)}>
      <rect x="5" y="10" width="14" height="10" rx="2" />
      <path d="M8 10V7a4 4 0 0 1 8 0v3" />
    </svg>
  );
}
