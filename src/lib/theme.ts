export type AppTheme = "light" | "dark";

export function resolveTheme(
  storedTheme: string | null | undefined,
  prefersDark: boolean,
): AppTheme {
  if (storedTheme === "light" || storedTheme === "dark") return storedTheme;
  return prefersDark ? "dark" : "light";
}
