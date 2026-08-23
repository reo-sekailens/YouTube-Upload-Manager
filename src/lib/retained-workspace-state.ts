import { useEffect, useState } from "react";
import type { Dispatch, SetStateAction } from "react";

const workspaceDrafts = new Map<string, unknown>();

/**
 * Keeps non-sensitive workspace drafts alive while inactive workspaces are
 * unmounted. Destructive confirmation text should intentionally remain in
 * ordinary component state so switching tabs clears it.
 */
export function useRetainedWorkspaceState<T>(
  key: string,
  initialValue: T | (() => T),
): [T, Dispatch<SetStateAction<T>>] {
  const [value, setValue] = useState<T>(() => {
    if (workspaceDrafts.has(key)) return workspaceDrafts.get(key) as T;
    return initialValue instanceof Function ? initialValue() : initialValue;
  });

  useEffect(() => {
    workspaceDrafts.set(key, value);
  }, [key, value]);

  return [value, setValue];
}
