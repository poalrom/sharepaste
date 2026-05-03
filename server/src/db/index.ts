import Database, { type Database as DbType } from "better-sqlite3";

export type Db = DbType;

export const openDb = (filePath: string): Db => {
  const db = new Database(filePath);
  db.pragma("journal_mode = WAL");
  db.pragma("foreign_keys = ON");
  db.pragma("synchronous = NORMAL");
  return db;
};
