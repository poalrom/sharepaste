import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["tests/**/*.test.ts"],
    pool: "threads",
    poolOptions: { threads: { singleThread: false } },
    testTimeout: 10000,
  },
});
