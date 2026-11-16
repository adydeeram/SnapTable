// Small inline icon set (16px grid, currentColor). Kept local: no icon-font/CDN.

type P = { className?: string; size?: number };
const svg = (path: React.ReactNode) =>
  function Icon({ className, size = 14 }: P) {
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 16 16"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.4}
        strokeLinecap="round"
        strokeLinejoin="round"
        className={className}
        aria-hidden
      >
        {path}
      </svg>
    );
  };

export const IconTable = svg(
  <>
    <rect x="2" y="3" width="12" height="10" rx="1.5" />
    <path d="M2 6.5h12M6 6.5V13" />
  </>,
);
export const IconView = svg(
  <>
    <rect x="2" y="3" width="12" height="10" rx="1.5" strokeDasharray="2 1.6" />
    <path d="M2 6.5h12" />
  </>,
);
export const IconDatabase = svg(
  <>
    <ellipse cx="8" cy="4" rx="5" ry="1.8" />
    <path d="M3 4v8c0 1 2.2 1.8 5 1.8s5-.8 5-1.8V4M3 8c0 1 2.2 1.8 5 1.8s5-.8 5-1.8" />
  </>,
);
export const IconChevron = svg(<path d="M6 4l4 4-4 4" />);
export const IconPlus = svg(<path d="M8 3v10M3 8h10" />);
export const IconPlay = svg(<path d="M5 3.5v9l7-4.5-7-4.5z" fill="currentColor" stroke="none" />);
export const IconStop = svg(<rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor" stroke="none" />);
export const IconRefresh = svg(<path d="M13 8a5 5 0 1 1-1.5-3.5M13 3v2.5h-2.5" />);
export const IconSearch = svg(
  <>
    <circle cx="7" cy="7" r="4" />
    <path d="M10 10l3.5 3.5" />
  </>,
);
export const IconClose = svg(<path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />);
export const IconQuery = svg(
  <>
    <path d="M3 3h10v10H3z" />
    <path d="M5.5 6.5L7 8l-1.5 1.5M8.5 10h2" />
  </>,
);
export const IconHistory = svg(
  <>
    <circle cx="8" cy="8" r="5.5" />
    <path d="M8 5v3l2 1.5" />
  </>,
);
export const IconEdit = svg(<path d="M10.5 3.5l2 2-7 7H3.5v-2l7-7z" />);
export const IconTrash = svg(<path d="M3 4.5h10M6 4.5V3h4v1.5M4.5 4.5l.7 8.5h5.6l.7-8.5" />);
export const IconPlug = svg(<path d="M6 2v3M10 2v3M4.5 5h7v2.5a3.5 3.5 0 0 1-7 0V5zM8 11v3" />);
export const IconSidebar = svg(
  <>
    <rect x="2" y="3" width="12" height="10" rx="1.5" />
    <path d="M6 3v10" />
  </>,
);
export const IconBolt = svg(<path d="M9 2L4 9h4l-1 5 5-7H8l1-5z" />);
export const IconChevronLeft = svg(<path d="M10 4L6 8l4 4" />);
export const IconChevronRight = svg(<path d="M6 4l4 4-4 4" />);
export const IconFilter = svg(<path d="M2.5 3.5h11l-4.2 5v4l-2.6 1.2V8.5z" />);
export const IconSidebarRight = svg(
  <>
    <rect x="2" y="3" width="12" height="10" rx="1.5" />
    <path d="M10 3v10" />
  </>,
);
