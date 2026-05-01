import { openDb } from "../db/index.js";
import { migrate } from "../db/migrate.js";
import { Repository } from "../db/repository.js";

const open = (dbPath: string) => {
  const db = openDb(dbPath);
  migrate(db);
  return new Repository(db);
};

export const runDeviceList = (args: { dbPath: string }) =>
  open(args.dbPath).memberships.listAll();

export const runDeviceRevoke = (args: { dbPath: string; deviceId: string }): void => {
  const repo = open(args.dbPath);
  const all = repo.memberships.listAll().filter((m) => m.device_id === args.deviceId);
  if (all.length === 0) throw new Error(`device ${args.deviceId} not found`);
  for (const m of all) {
    repo.memberships.revoke(m.user_id, m.device_id, Date.now());
  }
};
