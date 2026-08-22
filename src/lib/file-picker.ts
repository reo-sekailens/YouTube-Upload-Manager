/** Converts every platform's dialog result into a safe multi-file list. */
export function selectedFilePaths(selected: string | string[] | null): string[] {
  const paths = typeof selected === "string" ? [selected] : selected ?? [];
  return paths.filter((path) => path.trim().length > 0);
}
