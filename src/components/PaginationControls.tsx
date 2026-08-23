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
    <nav className="data-pagination" aria-label={`${label} pages`}>
      <span aria-live="polite">
        {start + 1}–{end} of {total}
      </span>
      <div>
        <button
          className="text-button"
          disabled={page <= 1}
          onClick={() => onPageChange(page - 1)}
          type="button"
        >
          Previous
        </button>
        <span>
          Page {page} of {pageCount}
        </span>
        <button
          className="text-button"
          disabled={page >= pageCount}
          onClick={() => onPageChange(page + 1)}
          type="button"
        >
          Next
        </button>
      </div>
    </nav>
  );
}
