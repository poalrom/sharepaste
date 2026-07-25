import { openDb } from "../db/index.js";
import { migrate } from "../db/migrate.js";
import { Repository } from "../db/repository.js";

/** Opens the database, runs `fn`, and always closes the handle. */
export const withRepo = <T>(dbPath: string, fn: (repo: Repository) => T): T => {
  const db = openDb(dbPath);
  try {
    migrate(db);
    return fn(new Repository(db));
  } finally {
    db.close();
  }
};
