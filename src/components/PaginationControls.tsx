import { ChevronLeft, ChevronRight } from "lucide-react";

type PaginationControlsProps = {
  label: string;
  page: number;
  pageCount: number;
  start: number;
  end: number;
  total: number;
  onPageChange: (page: number) => void;
};

export function PaginationControls({
  label,
  page,
  pageCount,
  start,
  end,
  total,
  onPageChange,
}: PaginationControlsProps) {
  if (total === 0) return null;
  return (
    <nav className="mt-2.5 flex items-center justify-between border-t border-[#e1e6ee] pt-2.5 text-[0.72rem] text-[#68748a] max-sm:flex-col max-sm:items-start max-sm:gap-2" aria-label={`${label} pages`}>
      <span aria-live="polite">
        {start + 1}–{end} of {total}
      </span>
      <div className="flex items-center gap-2">
        <button
          className="inline-flex cursor-pointer items-center gap-0.5 rounded px-1.5 py-1 text-[0.72rem] font-[680] text-[#344a67] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-[#2d68e847] focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-55"
          disabled={page <= 1}
          onClick={() => onPageChange(page - 1)}
          type="button"
        >
          <ChevronLeft aria-hidden="true" size={14} strokeWidth={2.25} />
          Previous
        </button>
        <span>
          Page {page} of {pageCount}
        </span>
        <button
          className="inline-flex cursor-pointer items-center gap-0.5 rounded px-1.5 py-1 text-[0.72rem] font-[680] text-[#344a67] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-[#2d68e847] focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-55"
          disabled={page >= pageCount}
          onClick={() => onPageChange(page + 1)}
          type="button"
        >
          Next
          <ChevronRight aria-hidden="true" size={14} strokeWidth={2.25} />
        </button>
      </div>
    </nav>
  );
}
