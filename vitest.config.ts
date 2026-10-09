import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Disposable source snapshots and rendered fixtures are verification
    // artifacts, never a second copy of the application's test suite.
    exclude: [...configDefaults.exclude, "output/**"],
    maxWorkers: 2,
  },
});
