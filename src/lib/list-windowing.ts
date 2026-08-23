export const DEFAULT_DATA_WINDOW_SIZE = 32;

export type DataWindow<T> = {
  items: T[];
  page: number;
  pageCount: number;
  start: number;
  end: number;
  total: number;
};

export function windowItems<T>(
  items: readonly T[],
  requestedPage: number,
  pageSize = DEFAULT_DATA_WINDOW_SIZE,
): DataWindow<T> {
  const safePageSize = Math.max(1, Math.floor(pageSize));
  const pageCount = Math.max(1, Math.ceil(items.length / safePageSize));
  const page = Math.min(pageCount, Math.max(1, Math.floor(requestedPage) || 1));
  const start = (page - 1) * safePageSize;
  const end = Math.min(items.length, start + safePageSize);
  return {
    items: items.slice(start, end),
    page,
    pageCount,
    start,
    end,
    total: items.length,
  };
}
