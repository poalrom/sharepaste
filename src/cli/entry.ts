import { openDb } from "../db/index.js";
import { migrate } from "../db/migrate.js";
import { Repository } from "../db/repository.js";

export const runEntryPurge = (args: { dbPath: string; userId: string }): number => {
  const db = openDb(args.dbPath);
  migrate(db);
  const repo = new Repository(db);
  return repo.entries.deleteAll(args.userId);
};
