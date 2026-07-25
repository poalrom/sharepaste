import { withRepo } from "./db.js";

export const runEntryPurge = (args: { dbPath: string; userId: string }): number =>
  withRepo(args.dbPath, (repo) => repo.entries.deleteAll(args.userId));
